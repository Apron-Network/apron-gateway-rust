use crate::helpers::respond_json;
use crate::state::{AppState, all, set, values};
use actix_web::web::{Data, HttpResponse, Json};
use actix_web::Error;
use async_std::channel;
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
    #[serde(default)]
    pub id: String,

    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub desc: String,

    pub base_url: String,

    pub schema: String,
}

#[derive(Deserialize, Debug, Serialize, PartialEq, Clone)]
pub struct ApronService {
    #[serde(default)]
    pub peer_id: String,

    // Uniq id for service, will be generated automatically after service is created
    pub id: String,

    // Name of the service
    pub name: String,

    #[serde(default)]
    pub desc: String,

    #[serde(default)]
    pub logo: String,

    #[serde(default)]
    pub usage: String,

    #[serde(default)]
    pub price_plan: String,

    #[serde(default)]
    pub user_id: String,

    pub providers: Vec<ApronServiceProvider>,

    #[serde(default)]
    pub is_deleted: bool,
}

pub struct SharedHandler {
    pub handler: Mutex<channel::Sender<String>>,
}

/// Create a service with data-raw.
pub async fn create_service(
    info: Json<ApronService>,
    data: AppState<ApronService>,
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

    println!(
        "[mgmt] new service: {}",
        serde_json::to_string(&new_service2).unwrap()
    );

    respond_json(new_service2)
}

/// Get All services
pub async fn get_services(data: AppState<ApronService>) -> HttpResponse {
    let hdata = values(data).unwrap();

    // for debug
    for (rcd) in &hdata {
        println!("{:?}", rcd);
    }
    HttpResponse::Ok().json(hdata)
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
