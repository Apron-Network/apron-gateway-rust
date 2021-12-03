use std::collections::HashMap;

use actix::Arbiter;
use actix_web::web::Data;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_web_actors::ws;
use futures::channel::mpsc;
use futures::channel::mpsc::{Receiver, Sender};
use futures::prelude::stream::Next;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use uuid::Bytes;

use crate::forward_service_actors::ClientSideWsActor;
use crate::forward_service_models::{HttpProxyResponse, ProxyRequestInfo};
use crate::network::Command;
use crate::state::{set, AppState};
use crate::{forward_service_actors, forward_service_utils::parse_request, PeerId, SharedHandler};
use crate::{helpers, network};

fn prepare_for_sending_p2p_transaction(
    query_args: web::Query<HashMap<String, String>>,
    raw_body: web::Bytes,
    req: HttpRequest,
    is_websocket: bool,
) -> (ProxyRequestInfo, PeerId) {
    // Parse request from client side
    let req_info = parse_request(query_args, raw_body, &req, is_websocket);

    // Generate peer id from seed
    // TODO: Replace this hard coded value to value fetched from service registration DB
    let (_, remote_peer_id) = helpers::generate_peer_id_from_seed(Some(1));

    (req_info, remote_peer_id)
}

pub(crate) async fn forward_http_proxy_request(
    query_args: web::Query<HashMap<String, String>>,
    raw_body: web::Bytes,
    req: HttpRequest,
    p2p_handler: Data<SharedHandler>,
    local_peer_id: Data<PeerId>,
    request_id_client_session_mapping: AppState<Sender<HttpProxyResponse>>,
) -> impl Responder {
    debug!("ClientSideGateway: Receive HTTP request: {:?}", req);

    let (req_info, remote_peer_id) =
        prepare_for_sending_p2p_transaction(query_args, raw_body, req, false);

    debug!("ClientSideGateway: Req info: {:?}", req_info);
    debug!("ClientSideGateway: remote peer: {:?}", remote_peer_id);

    let (resp_sender, mut resp_receiver): (Sender<HttpProxyResponse>, Receiver<HttpProxyResponse>) =
        mpsc::channel(0);

    // Save mapping between request_id and response sender,
    // which will be used to locate correct client session while getting response
    set(
        request_id_client_session_mapping.clone(),
        req_info.clone().request_id,
        resp_sender.clone(),
    );

    debug!(
        "ClientSideGateway: Fwd req id mapping: {:?}",
        request_id_client_session_mapping.as_ref()
    );

    // Send ProxyRequestInfo to service side gateway via stream
    let mut command_sender = p2p_handler.command_sender.lock().unwrap();
    command_sender
        .send(Command::SendRequest {
            peer: remote_peer_id,
            data: bincode::serialize(&req_info).unwrap(),
        })
        .await
        .unwrap();

    match resp_receiver.next().await {
        Some(resp) => {
            info!("Got HttpProxyResponse data");
            HttpResponse::Ok().body(resp.body)
        }
        _ => {
            error!("Got Non HttpProxyResponse data");
            HttpResponse::ServiceUnavailable().body("")
        }
    }
}

pub(crate) async fn forward_ws_proxy_request(
    query_args: web::Query<HashMap<String, String>>,
    // raw_body: web::Bytes,    // web::Bytes here may causes request parsing blocking, remove it for now
    req: HttpRequest,
    stream: web::Payload,
    p2p_handler: Data<SharedHandler>,
    local_peer_id: Data<PeerId>,
    request_id_client_session_mapping: AppState<Sender<HttpProxyResponse>>,
) -> impl Responder {
    info!("ClientSideGateway: Receive Websocket request: {:?}", req);

    let (req_info, remote_peer_id) =
        prepare_for_sending_p2p_transaction(query_args, web::Bytes::new(), req.clone(), true);

    info!("ClientSideGateway: Req info: {:?}", req_info);
    info!("ClientSideGateway: remote peer: {:?}", remote_peer_id);

    let (resp_sender, mut resp_receiver): (Sender<HttpProxyResponse>, Receiver<HttpProxyResponse>) =
        mpsc::channel(0);

    // Save mapping between request_id and response sender,
    // which will be used to locate correct client session while getting response
    set(
        request_id_client_session_mapping.clone(),
        req_info.clone().request_id,
        resp_sender.clone(),
    );

    info!(
        "ClientSideGateway: Fwd req id mapping: {:?}",
        request_id_client_session_mapping.as_ref()
    );


    // Create websocket session between ClientSideGateway and Client
    ws::start(
        ClientSideWsActor {
            req_info,
            remote_peer_id,
            p2p_handler,
            resp_receiver,
        },
        &req,
        stream,
    )

    // let proxy_resp = resp_receiver.recv().unwrap();
    //
    // HttpResponse::Ok().body(proxy_resp.body)
}
