use crate::helpers::respond_json;
use crate::network::{Command, Event};
use crate::state::{self, all, delete, get, AppState};
use actix_web::web::{Data, HttpResponse, Json};
use actix_web::Error;
use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use serde::Serialize;
use std::sync::Mutex;

use libp2p::PeerId;
// use serde_derive::Deserialize;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize, Debug, Serialize, PartialEq, Clone)]
pub struct ApronServiceProvider {
    pub id: Option<String>,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub base_url: Option<String>,
    pub schema: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub extra_detail: Option<String>,
}

#[derive(Deserialize, Debug, Serialize, PartialEq, Clone)]
pub struct ApronService {
    pub peer_id: Option<String>,
    // Uniq id for service, will be generated automatically
    pub id: String,
    // hostname provides this service, will be used to search service while forwarding requesting.
    pub domain_name: Option<String>,

    pub providers: Option<Vec<ApronServiceProvider>>,

    pub is_deleted: Option<bool>,
}

impl ApronService {
    pub fn update(&mut self, other: ApronService) {
        if other.peer_id.is_some() {
            self.peer_id = other.peer_id;
        }
        if other.domain_name.is_some() {
            self.domain_name = other.domain_name;
        }
        if other.is_deleted.is_some() {
            self.is_deleted = other.is_deleted;
        }
        // update ApronServiceProvider
        if other.providers.is_some() {
            if self.providers.is_some() {
                let mut providers = self.providers.clone().unwrap();
                for other_provider in other.providers.clone().unwrap() {
                    let mut found = false;
                    let other_id = other_provider.clone().id;
                    for i in 0..providers.len() {
                        if providers[i].id == other_id {
                            providers[i].update(other_provider.clone());
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        providers.push(other_provider.clone());
                    }
                }
                self.providers = Some(providers);
            } else {
                self.providers = other.providers;
            }
        }
    }
}

impl ApronServiceProvider {
    pub fn update(&mut self, other: ApronServiceProvider) {
        if other.id.is_some() {
            self.id = other.id;
        }
        if other.name.is_some() {
            self.name = other.name;
        }
        if other.desc.is_some() {
            self.desc = other.desc;
        }
        if other.base_url.is_some() {
            self.base_url = other.base_url;
        }
        if other.schema.is_some() {
            self.schema = other.schema;
        }
        if other.created_at.is_some() {
            self.created_at = other.created_at;
        }
        if other.updated_at.is_some() {
            self.updated_at = other.updated_at;
        }
        if other.extra_detail.is_some() {
            self.extra_detail = other.extra_detail;
        }
    }
}

// #[derive(Debug,Serialize, PartialEq, Clone)]
pub struct SharedHandler {
    pub command_sender: Mutex<mpsc::Sender<Command>>,
    // pub event_reciver: Mutex<mpsc::Receiver<Event>>,
}

/// Create a service with data-raw.
pub async fn new_update_service(
    info: Json<ApronService>,
    data: AppState<ApronService>,
    p2p_handler: Data<SharedHandler>,
    local_peer_id: Data<PeerId>,
) -> Result<Json<ApronService>, Error> {
    let key = info.id.clone();
    let mut new_service = info.into_inner();

    let service = state::get(data.clone(), key.clone());
    // check create or update
    if service.is_some() {
        let mut service = service.unwrap();
        service.update(new_service);
        state::set(data, key.clone(), service.clone());

        // publish data to the whole p2p network
        let mut command_sender = p2p_handler.command_sender.lock().unwrap();
        let message = serde_json::to_string(&service).unwrap();
        command_sender
            .send(Command::PublishGossip {
                data: message.into_bytes(),
            })
            .await
            .unwrap();

        println!(
            "[mgmt] update service: {}",
            serde_json::to_string(&service).unwrap()
        );
        respond_json(service)
    } else {
        new_service.peer_id = Some(local_peer_id.clone().to_base58());

        let mut new_service2 = new_service.clone();
        new_service2.peer_id = Some(local_peer_id.clone().to_base58());

        state::set(data, key, new_service);

        // publish data to the whole p2p network
        let mut command_sender = p2p_handler.command_sender.lock().unwrap();
        let message = serde_json::to_string(&new_service2).unwrap();
        command_sender
            .send(Command::PublishGossip {
                data: message.into_bytes(),
            })
            .await
            .unwrap();

        println!(
            "[mgmt] new service: {}",
            serde_json::to_string(&new_service2).unwrap()
        );

        respond_json(new_service2)
    }
}

/// Delete a service with data-raw.
pub async fn delete_service(
    info: Json<ApronService>,
    data: AppState<ApronService>,
    p2p_handler: Data<SharedHandler>,
) -> HttpResponse {
    let key = info.id.clone();
    let service = state::delete(data, key.clone());
    match service {
        Some(service) => {
            let mut new_service = service.clone();
            new_service.is_deleted = Some(true);

            // state::delete(data.clone(), key.clone());
            println!("[mgmt] delete service: {}", key);

            // publish data to the whole p2p network
            let mut command_sender = p2p_handler.command_sender.lock().unwrap();
            let message = serde_json::to_string(&new_service).unwrap();
            command_sender
                .send(Command::PublishGossip {
                    data: message.into_bytes(),
                })
                .await
                .unwrap();

            HttpResponse::Ok().body("")
        }
        None => HttpResponse::NotFound().body(""),
    }
}

/// Get All services
pub async fn get_services(data: AppState<ApronService>) -> HttpResponse {
    let hdata = all(data).unwrap();

    // for debug
    // for (key, value) in &hdata {
    //     println!("{}: {}", key, value.id);
    // }
    HttpResponse::Ok().body(serde_json::to_string(&hdata).unwrap())
}

pub async fn list_local_services(
    data: AppState<ApronService>,
    local_peer_id: Data<PeerId>,
) -> HttpResponse {
    println!("[mgmt]: List Local Available Service");
    let mut response: Vec<ApronService> = Vec::new();
    let hdata = all(data).unwrap();
    let peer_id = Some(local_peer_id.clone().to_base58());
    for value in hdata.values() {
        if value.peer_id == peer_id {
            // local service
            response.push(value.clone());
        }
    }
    HttpResponse::Ok().body(serde_json::to_string(&response).unwrap())
}

pub async fn list_remote_services(
    data: AppState<ApronService>,
    local_peer_id: Data<PeerId>,
) -> HttpResponse {
    println!("[mgmt]: List Remote Available Service");
    let mut response: Vec<ApronService> = Vec::new();
    let hdata = all(data).unwrap();
    let peer_id = Some(local_peer_id.clone().to_base58());
    for value in hdata.values() {
        if value.peer_id != peer_id {
            // local service
            response.push(value.clone());
        }
    }
    HttpResponse::Ok().body(serde_json::to_string(&response).unwrap())
}

pub async fn list_service_peers(data: AppState<ApronService>) -> HttpResponse {
    println!("[mgmt]: List Available Service Peers");
    let mut response = HashSet::new();
    let hdata = all(data).unwrap();
    for value in hdata.values() {
        response.insert(value.peer_id.clone());
    }
    HttpResponse::Ok().body(serde_json::to_string(&response).unwrap())
}
