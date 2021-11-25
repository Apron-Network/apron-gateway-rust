use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc::Sender;
use std::sync::Mutex;

use actix::{ContextFutureSpawner, Response};
use actix_web::{App, HttpResponse, HttpServer, web, web::Data};
use actix_web::body::{Body, ResponseBody};
use async_std::channel;
use async_std::task::block_on;
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use libp2p::{Multiaddr, multiaddr::Protocol, PeerId};
use serde::de::Unexpected::Str;
use structopt::StructOpt;

use crate::forward_service_models::HttpProxyResponse;
use crate::forward_service_utils::{send_http_request, send_http_request_blocking};
use crate::Protocol::Http;
use crate::routes::routes;
use crate::service::{ApronService, SharedHandler};
use crate::state::{get, new_state};

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

    // Spawn away the event loop that will keep the swarm going.
    async_std::task::spawn(network::network_event_loop(
        swarm,
        command_receiver,
        event_sender,
        data.clone(),
    ));

    let p2p_handler = Data::new(SharedHandler {
        command_sender: Mutex::new(command_sender),
        // event_reciver: Mutex::new(event_receiver),
    });

    let req_id_client_session_mapping = Data::new(Mutex::new(HashMap::new()));

    forward_service::ForwardService {
        port: opt.forward_port,
        p2p_handler: p2p_handler.clone(),
        peer_id,
        req_id_client_session_mapping: req_id_client_session_mapping.clone(),
    }
        .start()
        .await;

    let mgmt_local_peer_id = web::Data::new(peer_id.clone());

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .app_data(p2p_handler.clone())
            .app_data(mgmt_local_peer_id.clone())
            .configure(routes)
    })
        .bind(format!("0.0.0.0:{}", opt.mgmt_port))?
        .run();

    loop {
        match event_receiver.next().await {
            Some(network::Event::ProxyRequst { info }) => {
                println!("Proxy request is {:?}", info.clone().request_id);

                let tmp_base = "https://webhook.site/7b86ef43-5748-4a00-8f14-3ccaaa4d6253";
                let body = send_http_request_blocking(info.clone(), Some(tmp_base)).unwrap();

                println!(
                    "Response data for request {} is {:?}",
                    info.request_id, body
                );

                println!("sender mapping: {:?}", req_id_client_session_mapping.clone());

                // Get sender from saved mapping
                let sender = get(
                    req_id_client_session_mapping.clone(),
                    info.clone().request_id,
                )
                    .unwrap();

                let mut headers: HashMap<String, Vec<u8>> = HashMap::new();

                let http_proxy_response = HttpProxyResponse {
                    status_code: 200,
                    headers,
                    body: body.into(),
                };

                sender.send(http_proxy_response).unwrap();
            }
            _ => todo!(),
        }
    }

    // future::try_join(mgmt_service, fwd_service).await?;

    Ok(())
}
