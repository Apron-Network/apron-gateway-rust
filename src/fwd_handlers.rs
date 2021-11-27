use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

use actix_web::web::Data;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_web_actors::ws;
use log::debug;

use futures::SinkExt;
use uuid::Bytes;

use crate::forward_service_models::HttpProxyResponse;
use crate::helpers;
use crate::network::Command;
use crate::state::{set, AppState};
use crate::{
    forward_service_actors,
    forward_service_utils::{parse_request},
    PeerId, SharedHandler,
};

// TODO: This should be invoked in service side gw
pub(crate) async fn forward_http_proxy_request(
    query_args: web::Query<HashMap<String, String>>,
    raw_body: web::Bytes,
    req: HttpRequest,
    p2p_handler: Data<SharedHandler>,
    local_peer_id: Data<PeerId>,
    request_id_client_session_mapping: AppState<Sender<HttpProxyResponse>>,
) -> impl Responder {
    println!("Local peer id: {:?}", local_peer_id);
    // TODO: Split http and websocket

    // Parse request from client side
    let req_info = parse_request(query_args, raw_body, &req);

    // For p2p environment, the req_info should be sent to service side gateway via stream
    let mut command_sender = p2p_handler.command_sender.lock().unwrap();

    // TODO: Hard coded, should be replaced with registered service data
    let (remote_key, remote_peer_id) = helpers::generate_peer_id_from_seed(Some(1));

    println!("[fwd] Request info: {:?} to {}", &req_info, remote_peer_id);

    let (resp_sender, resp_receiver): (Sender<HttpProxyResponse>, Receiver<HttpProxyResponse>) =
        mpsc::channel();

    set(
        request_id_client_session_mapping.clone(),
        req_info.clone().request_id,
        resp_sender.clone(),
    );

    println!(
        "Fwd req id mapping: {:?}",
        request_id_client_session_mapping.as_ref()
    );

    // Send ProxyRequestInfo to service side gateway via stream
    command_sender
        .send(Command::SendRequest {
            // peer: PeerId::from_str(local_peer_id.as_str()).unwrap(),
            peer: remote_peer_id,
            data: bincode::serialize(&req_info).unwrap(),
        })
        .await
        .unwrap();

    let proxy_resp = resp_receiver.recv().unwrap();

    HttpResponse::Ok().body(proxy_resp.body)
}

pub(crate) async fn forward_ws_proxy_request(
    query_args: web::Query<HashMap<String, String>>,
    req: HttpRequest,
    stream: web::Payload,
    p2p_handler: Data<SharedHandler>,
) -> impl Responder {
    let req_info = parse_request(query_args, web::Bytes::new(), &req);
    debug!("ClientSideGateway: Req info: {:?}", req_info);
    // TODO: Change to configured ws server addr
    let resp = ws::start(
        forward_service_actors::ClientSideWsActor {
            service_uri: "ws://localhost:10000",
            addr: None,
        },
        &req,
        stream,
    );
    println!("Resp: {:?}", resp);
    resp
}
