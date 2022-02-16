use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use actix_web::web::{Data, HttpResponse, Json};
use actix_web::Error;
use futures::channel::mpsc;
use futures::SinkExt;
use libp2p::PeerId;
use serde::Deserialize;
use serde::Serialize;

use crate::contract::{add_service, call, exec};
use crate::helpers::respond_json;
use crate::network::Command;
use crate::state::{all, set, values, AppState};
use crate::usage_report::UsageReport;
use crate::UsageReportManager;

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

    pub name: Option<String>,
    pub desc: Option<String>,
    pub logo: Option<String>,
    pub usage: Option<String>,

    pub providers: Option<Vec<ApronServiceProvider>>,

    pub is_deleted: Option<bool>,

    pub price_plan: Option<String>,

    pub user_id: Option<String>,
}

impl ApronService {
    // service - serviceprovide in a 1-1 relationship
    pub fn apronservice_to_args(self) -> Vec<String> {
        let provider = self.providers.unwrap()[0].clone();

        let created_at = provider
            .created_at
            .map_or(String::from("123456789"), |ts| ts.to_string());

        let provider_name = provider.name.unwrap_or(String::from(""));
        let schema = provider.schema.unwrap_or_default();
        let extra_detail = provider.extra_detail.unwrap_or_default();
        let price_plan = self.price_plan.unwrap_or_default();
        let name = self.name.unwrap_or_default();
        let provider_desc = provider.desc.unwrap_or_default();
        let logo = self.logo.unwrap_or_default();

        vec![
            format!("\"{}\"", self.id),       //uuid
            format!("\"{}\"", name),          //name
            format!("\"{}\"", provider_desc), //desc
            format!("\"{}\"", logo),          //logo
            format!("\"{}\"", created_at),    //createTime
            format!("\"{}\"", provider_name), //providerName
            format!("\"{}\"", "5F7Xv7RaJe8BBNULSuRTXWtfn68njP1NqQL5LLf41piRcEJJ"), //providerOwner
            format!("\"{}\"", String::from("")), //usage
            format!("\"{}\"", schema),        //schema
            format!("\"{}\"", price_plan),    //pricePlan
            format!("\"{}\"", extra_detail),  //declaimer
        ]
    }

    pub fn update(&mut self, other: ApronService) {
        if other.price_plan.is_some() {
            self.price_plan = other.price_plan;
        }
        if other.user_id.is_some() {
            self.user_id = other.user_id;
        }
        if other.peer_id.is_some() {
            self.peer_id = other.peer_id;
        }
        if other.name.is_some() {
            self.name = other.name;
        }
        if other.is_deleted.is_some() {
            self.is_deleted = other.is_deleted;
        }
        if other.desc.is_some() {
            self.desc = other.desc;
        }
        if other.logo.is_some() {
            self.logo = other.logo;
        }
        if other.usage.is_some() {
            self.usage = other.usage;
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

    pub fn get_http_provider(&self) -> Option<String> {
        self.get_provider(String::from("http"))
    }

    pub fn get_ws_provider(&self) -> Option<String> {
        self.get_provider(String::from("ws"))
    }

    fn get_provider(&self, schema: String) -> Option<String> {
        let mut rslt: Option<String> = None;
        if self.providers.is_some() {
            for p in self.providers.as_ref().unwrap() {
                if p.schema.is_some() && p.schema.as_ref().unwrap().starts_with(&schema) {
                    rslt = Option::from(String::from(format!(
                        "{}://{}",
                        p.schema.as_ref().unwrap(),
                        p.base_url.as_ref().unwrap()
                    )));
                    break;
                }
            }
        }

        rslt
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

// #[derive(Debug, Serialize, PartialEq, Clone)]
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

    let service = crate::state::get(data.clone(), key.clone());
    // check create or update
    if service.is_some() {
        let mut service = service.unwrap();
        service.update(new_service);
        crate::state::set(data, key.clone(), service.clone());

        // publish data to the whole p2p network
        let mut command_sender = p2p_handler.command_sender.lock().unwrap();
        let message = serde_json::to_string(&service).unwrap();
        command_sender
            .send(Command::PublishGossip {
                data: message.into_bytes(),
            })
            .await
            .unwrap();

        command_sender
            .send(Command::AddService {
                args: service.clone().apronservice_to_args(),
            })
            .await
            .unwrap();

        // crate::contract::add_service(
        //     "ws://127.0.0.1:9944".to_string(),
        //     "5FwVe1jsNQociUBV17VZs6SxcWbYy8JjULj9KNuY4gvb43uK".to_string(),
        //     "./release/services_market.json".to_string(),
        //     service.clone().apronservice_to_args(),
        // );

        println!(
            "[mgmt] update service: {}",
            serde_json::to_string(&service).unwrap()
        );

        respond_json(service)
    } else {
        new_service.peer_id = Some(local_peer_id.clone().to_base58());

        let mut new_service2 = new_service.clone();
        new_service2.peer_id = Some(local_peer_id.clone().to_base58());

        crate::state::set(data, key, new_service);

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

        command_sender
            .send(Command::AddService {
                args: new_service2.clone().apronservice_to_args(),
            })
            .await
            .unwrap();

        // crate::contract::add_service(
        //     "ws://127.0.0.1:9944".to_string(),
        //     "5FwVe1jsNQociUBV17VZs6SxcWbYy8JjULj9KNuY4gvb43uK".to_string(),
        //     "./release/services_market.json".to_string(),
        //     new_service2.clone().apronservice_to_args(),
        // );

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
    let service = crate::state::delete(data, key.clone());
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
    println!("[mgmt]: List All Available Service");
    let hdata = values(data).unwrap();

    // for debug
    // for (key, value) in &hdata {
    //     println!("{}: {}", key, value.id);
    // }
    HttpResponse::Ok().json(hdata)
}

pub async fn get_usage_reports(data: Data<UsageReportManager>) -> HttpResponse {
    println!("[mgmt]: Get All Usage Report");
    let report_mgr = Some(data.clone());
    let mut report: HashMap<String, UsageReport> = HashMap::new();
    if report_mgr.is_some() {
        report.extend(report_mgr.clone().unwrap().account_reports.clone());
        report_mgr.clone().unwrap().account_reports.clone().clear();
    }
    println!("Usage reports: {:?}", report);
    HttpResponse::Ok().finish()
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
    // HttpResponse::Ok().body(serde_json::to_string(&response).unwrap())
    HttpResponse::Ok().json(response)
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
    // HttpResponse::Ok().body(serde_json::to_string(&response).unwrap())
    HttpResponse::Ok().json(response)
}

pub async fn list_service_peers(data: AppState<ApronService>) -> HttpResponse {
    println!("[mgmt]: List Available Service Peers");
    let mut response = HashSet::new();
    let hdata = all(data).unwrap();
    for value in hdata.values() {
        response.insert(value.peer_id.clone());
    }
    // HttpResponse::Ok().body(serde_json::to_string(&response).unwrap())
    HttpResponse::Ok().json(response)
}
