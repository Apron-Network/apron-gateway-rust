use crate::helpers::{respond_json, respond_ok};
use crate::state::{AppState,set,get,all};
use actix_web::web::{Data, HttpResponse, Json, Path};
use actix_web::Error;
use serde::Serialize;
use uuid::Uuid;
use std::sync::Mutex;
// use futures::channel::{mpsc, oneshot};
// use futures::SinkExt;
use async_std::channel;

use libp2p::gossipsub::{
  Gossipsub, GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};

use libp2p::{Swarm, gossipsub, identity, swarm::SwarmEvent, PeerId};

#[derive(Debug,Serialize, PartialEq, Clone)]
pub struct ApronService {
  pub id: String,
}

// #[derive(Debug,Serialize, PartialEq, Clone)]
pub struct SharedHandler {
  pub handler: Mutex<channel::Sender<String>>,
}

/// Create a service
pub async fn create_service(data: AppState::<ApronService>, p2p_handler: Data<SharedHandler>) -> Result<Json<ApronService>, Error> {

  let key = Uuid::new_v4().to_string();

  let new_service = ApronService {
    id: key.clone()
  };

  let new_service2 = new_service.clone();

  set(data, key, new_service);

  // @ToDo publish data to the whole p2p network
  // let mut handler = p2p_handler.handler.lock().unwrap();
  // let topic = Topic::new("test-net");
  // handler.behaviour_mut().publish(topic, "new_service2".as_bytes());
   let mut sender = p2p_handler.handler.lock().unwrap();
  sender.send("new servece".to_string()).await.unwrap();

  println!("new service: {}", new_service2.id.clone());

  respond_json(new_service2)
  // respond_json(get(data.clone(),key2).unwrap())
}

/// Get All services
pub async fn get_services(data: AppState::<ApronService>) -> HttpResponse {

  let hdata = all(data).unwrap();

  // for debug
  for(key, value) in &hdata {
    println!("{}: {}", key, value.id);
  }
  HttpResponse::Ok().body(serde_json::to_string(&hdata).unwrap())
}