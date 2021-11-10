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
) -> Result<Json<ApronService>, Error> {
    let key = info.id.clone();
    let new_service = info.into_inner();

    println!(
        "new service:{}",
        serde_json::to_string(&new_service).unwrap()
    );

    // let new_service = ApronService {
    //   id: info.id,
    //   domain_name: info.
    // };

    let new_service2 = new_service.clone();

    set(data, key, new_service);

    // publish data to the whole p2p network
    let mut sender = p2p_handler.handler.lock().unwrap();
    let message = serde_json::to_string(&new_service2).unwrap();
    sender
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
