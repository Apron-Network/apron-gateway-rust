use crate::helpers::{respond_json, respond_ok};
use crate::state::{AppState,set,get,all};
use actix_web::web::{Data, HttpResponse, Json, Path};
use actix_web::Error;
use serde::Serialize;
use uuid::Uuid;
#[derive(Debug,Serialize, PartialEq, Clone)]
pub struct ApronService {
  id: String,
}

/// Create a service
pub async fn create_service(data: AppState::<ApronService>) -> Result<Json<ApronService>, Error> {

  let key = Uuid::new_v4().to_string();

  let new_service = ApronService {
    id: key.clone()
  };

  let new_service2 = new_service.clone();

  set(data, key, new_service);

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