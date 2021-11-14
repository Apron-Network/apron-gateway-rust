use crate::helpers::{respond_json, respond_ok};
use crate::network::Command;
use crate::state::{all, get, set, AppState};
use actix_web::web::{Data, HttpResponse, Json, Path};
use actix_web::Error;
use serde::Serialize;
use std::sync::Mutex;
use uuid::Uuid;
// use futures::channel::{mpsc, oneshot};
// use futures::SinkExt;
use async_std::channel;

use libp2p::gossipsub::{
    Gossipsub, GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity,
    ValidationMode,
};

use libp2p::{gossipsub, identity, swarm::SwarmEvent, PeerId, Swarm};
// use serde_derive::Deserialize;
use serde::Deserialize;
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Deserialize, Debug, Serialize, PartialEq, Clone)]
pub struct ApronServiceProvider {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub base_url: String,
    pub schema: String,
    // pub created_at: i64,
    // pub updated_at: i64,
    // pub extra_detail: String,
}

#[derive(Deserialize, Debug, Serialize, PartialEq, Clone)]
pub struct ApronService {
    pub peer_id: String,
    // Uniq id for service, will be generated automatically
    pub id: String,
    // hostname provides this service, will be used to search service while forwarding requesting.
    pub domain_name: String,

    pub providers: Vec<ApronServiceProvider>,

    pub is_deleted: bool,
}

// #[derive(Debug,Serialize, PartialEq, Clone)]
pub struct SharedHandler {
    pub handler: Mutex<channel::Sender<Command>>,
}

/// Create a service with data-raw.
pub async fn create_service(
    info: Json<ApronService>,
    data: AppState<ApronService>,
    p2p_handler: Data<SharedHandler>,
    local_peer_id: Data<PeerId>,
) -> Result<Json<ApronService>, Error> {
    let key = info.id.clone();
    let mut new_service = info.into_inner();
    new_service.peer_id = local_peer_id.clone().to_base58();

    println!(
        "new service:{}",
        serde_json::to_string(&new_service).unwrap()
    );

    let mut new_service2 = new_service.clone();
    new_service2.peer_id = local_peer_id.clone().to_base58();

    set(data, key, new_service);

    // publish data to the whole p2p network
    let command_sender = p2p_handler.handler.lock().unwrap();
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

/// Get All services
pub async fn get_services(data: AppState<ApronService>) -> HttpResponse {
    let hdata = all(data).unwrap();

    // for debug
    for (key, value) in &hdata {
        println!("{}: {}", key, value.id);
    }
    HttpResponse::Ok().body(serde_json::to_string(&hdata).unwrap())
}

pub async fn list_local_services(
    data: AppState<ApronService>,
    local_peer_id: Data<PeerId>,
) -> HttpResponse {
    println!("[mgmt]: List Local Available Service");
    let mut response: Vec<ApronService> = Vec::new();
    let hdata = all(data).unwrap();
    let peer_id = local_peer_id.clone().to_base58();
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
    let peer_id = local_peer_id.clone().to_base58();
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
