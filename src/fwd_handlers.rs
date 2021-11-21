use std::collections::HashMap;

use actix_web::web::Data;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_web_actors::ws;
use log::debug;

use futures::SinkExt;

use crate::helpers;
use crate::network::Command;
use crate::{
    forward_service_actors,
    forward_service_utils::{parse_request, send_http_request},
    PeerId, SharedHandler,
};

// TODO: This should be invoked in service side gw
pub(crate) async fn forward_http_proxy_request(
    query_args: web::Query<HashMap<String, String>>,
    raw_body: web::Bytes,
    req: HttpRequest,
    p2p_handler: Data<SharedHandler>,
    local_peer_id: Data<PeerId>,
) -> impl Responder {
    println!("Local peer id: {:?}", local_peer_id);
    // Parse request from client side
    // TODO: Split http and websocket
    let req_info = parse_request(query_args, raw_body, &req);

    // For p2p environment, the req_info should be sent to service side gateway via stream
    let mut command_sender = p2p_handler.command_sender.lock().unwrap();

    // TODO: Hard coded, should be replaced with registered service data
    let (remote_key, remote_peer_id) = helpers::generate_peer_id_from_seed(Some(1));

    println!("[fwd] Request info: {:?} to {}", &req_info, remote_peer_id);

    // Send ProxyRequestInfo to service side gateway via stream
    command_sender
        .send(Command::SendRequest {
            // peer: PeerId::from_str(local_peer_id.as_str()).unwrap(),
            peer: remote_peer_id,
            data: bincode::serialize(&req_info).unwrap(),
        })
        .await
        .unwrap();

    // Build request sent to forwarded service
    // let mut foo = async move {send_http_request(req_info, None).await.unwrap()};
    // let bar = foo.await.take_body();

    // let client_req = send_http_request(req_info, None);
    // let resp_body = client_req.unwrap().send().await.unwrap().body().await.unwrap().to_vec();

    // send_http_request(req_info, None).await.unwrap()

    // TODO: missing fn: pass response back to client side gateway
    // TODO: missing fn: pass response sent from service side gateway, and respond to client

    HttpResponse::Ok()
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
