use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;
use std::task;

use actix::{spawn, Addr, Arbiter, ContextFutureSpawner, Response};
use actix_web::body::{Body, ResponseBody};
use actix_web::{web, web::Data, App, HttpResponse, HttpServer};
use async_std::task::block_on;
use awc::http::header::fmt_comma_delimited;
use awc::http::Uri;
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use libp2p::gossipsub::Topic;
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
use log::info;
use serde::de::Unexpected::Str;
use structopt::StructOpt;

use crate::forward_service_actors::ServiceSideWsActor;
// use crate::event_loop::EventLoop;
use crate::forward_service_models::HttpProxyResponse;
use crate::forward_service_utils::{connect_to_ws_service, send_http_request_blocking};
use crate::network::{Command, DataExchangeRequest, Event};
use crate::routes::routes;
use crate::service::{ApronService, SharedHandler};
use crate::state::{get, new_state};
use crate::Protocol::Http;

// mod event_loop;
mod forward_service;
mod forward_service_actors;
mod forward_service_models;
mod forward_service_utils;
mod fwd_handlers;
mod helpers;
mod network;
mod routes;
mod service;
mod state;

#[derive(Debug, StructOpt)]
#[structopt(name = "apron gateway")]
struct Opt {
    /// Fixed value to generate deterministic peer ID.
    #[structopt(long)]
    peer: Option<Multiaddr>,

    #[structopt(default_value = "2145", long)]
    p2p_port: i32,

    #[structopt(default_value = "8080", long)]
    forward_port: i32,

    #[structopt(default_value = "8082", long)]
    mgmt_port: i32,

    #[structopt(default_value = "apron-test-net", long)]
    rendezvous: String,

    /// Fixed value to generate deterministic peer ID.
    #[structopt(long)]
    secret_key_seed: Option<u8>,
}

#[actix_web::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let opt = Opt::from_args();

    let mut swarm = network::new(opt.secret_key_seed).await.unwrap();

    // In case the user provided an address of a peer on the CLI, dial it.
    if let Some(to_dial) = opt.peer {
        let dialing = to_dial.clone();
        let peer_id = match dialing.iter().last() {
            Some(Protocol::P2p(hash)) => PeerId::from_multihash(hash).expect("Valid hash."),
            _ => return Err("Expect peer multiaddr to contain peer ID.".into()),
        };
        swarm
            .behaviour_mut()
            .kademlia
            .add_address(&peer_id, to_dial.clone());
        match swarm.dial_addr(to_dial) {
            Ok(_) => println!("Dialed {:?}", dialing),
            Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
        };
    };

    // Listen on all interfaces and whatever port the OS assigns
    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{}", opt.p2p_port)
                .parse()
                .unwrap(),
        )
        .unwrap();

    let peer_id = swarm.local_peer_id().clone();

    let (command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, mut event_receiver) = mpsc::channel(0);

    let data = new_state::<ApronService>();
    // let service_peer_mapping = new_state::<PeerId>();

    let topic = String::from("apron-test-net");

    // let req_id_client_session_mapping = Data::new(Mutex::new(mpsc::Sender<HttpProxyResponse>));
    let req_id_client_session_mapping = new_state::<mpsc::Sender<HttpProxyResponse>>();

    async_std::task::spawn(network::network_event_loop(
        swarm,
        command_receiver,
        event_sender,
        data.clone(),
        req_id_client_session_mapping.clone(),
    ));

    let p2p_handler = Data::new(SharedHandler {
        command_sender: Mutex::new(command_sender),
        // event_reciver: Mutex::new(event_receiver),
    });

    let fwd_service = forward_service::ForwardService {
        port: opt.forward_port,
        p2p_handler: p2p_handler.clone(),
        peer_id,
        req_id_client_session_mapping: req_id_client_session_mapping.clone(),
    }
    .start();

    let mgmt_local_peer_id = web::Data::new(peer_id.clone());
    let mgmt_p2p_handler = p2p_handler.clone();

    let mgmt_service = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .app_data(mgmt_p2p_handler.clone())
            .app_data(mgmt_local_peer_id.clone())
            .configure(routes)
    })
    .bind(format!("0.0.0.0:{}", opt.mgmt_port))?
    .run();

    // Websocket connect loop, which handles those actions:
    // * receives message sent from network event handler,
    // * processes websocket connection
    // * forward message to network handler with data_sender
    Arbiter::spawn(async move {
        let mut req_id_addr_mapping: HashMap<String, Addr<ServiceSideWsActor>> = HashMap::new();
        loop {
            futures::select! {
                evt = event_receiver.next() => {
                    info!("Receive event: {:#?}", evt);
                    match evt {
                        Some(evt) => match evt {
                            network::Event::ProxyRequestToMainLoop {
                                info,
                                remote_peer_id,
                                data_sender,
                            } => {
                                info!(
                                    "ServiceSideGateway: Proxy request received is {:?}",
                                    info.clone().request_id
                                );
                                // TODO: Get ws url from saved service info
                                let addr = connect_to_ws_service(
                                    "ws://localhost:10000",
                                    remote_peer_id,
                                    info.clone().request_id,
                                    data_sender,
                                ).await;
                                req_id_addr_mapping.insert(info.clone().request_id, addr);
                                info!("ServiceSideGateway: InitWsConn: req_id_addr_mapping keys: {:?}, request_id: {:?}", req_id_addr_mapping.keys(), info.clone().request_id);
                            }

                            network::Event::ProxyDataFromClient {
                                data,
                                mut data_sender,
                            } => {
                                info!(
                                    "ServiceSideGateway: client side proxy data received {:?}",
                                    data.data.clone()
                                );
                                // TODO: Send data to websocket connection
                                info!("ServiceSideGateway: WsData: req_id_addr_mapping keys: {:?}, request_id: {:?}", req_id_addr_mapping.keys(), data.request_id.clone());
                                let service_addr = req_id_addr_mapping.get(&data.request_id).unwrap();
                                service_addr.do_send(data);
                            }
                            _ => {
                                info!("Receive event 1: {:#?}", evt);
                            }
                        }
                        _ => {
                            info!("Receive event 2: {:#?}", evt);
                        }
                    }
                }
            }
        }
    });

    future::try_join(mgmt_service, fwd_service).await?;

    Ok(())
}
