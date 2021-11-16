use std::error::Error;
use std::sync::Mutex;

use actix_web::{web, web::Data, App, HttpServer};
use async_std::channel;
use futures::prelude::*;
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
use structopt::StructOpt;

use crate::routes::routes;
use crate::service::{ApronService, SharedHandler};
use crate::state::new_state;

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

    let (command_sender, command_receiver) = channel::unbounded();

    let data = new_state::<ApronService>();
    // let service_peer_mapping = new_state::<PeerId>();

    // Spawn away the event loop that will keep the swarm going.
    async_std::task::spawn(network::network_event_loop(
        swarm,
        command_receiver,
        data.clone(),
    ));

    let p2p_handler = Data::new(SharedHandler {
        handler: Mutex::new(command_sender),
    });

    let fwd_service = forward_service::ForwardService {
        port: opt.forward_port,
        p2p_handler: p2p_handler.clone(),
        peer_id,
    }
    .start();

    let mgmt_local_peer_id = web::Data::new(peer_id.clone());
    let mgmt_service = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .app_data(p2p_handler.clone())
            .app_data(mgmt_local_peer_id.clone())
            .configure(routes)
    })
    .bind(format!("0.0.0.0:{}", opt.mgmt_port))?
    .run();

    future::try_join(mgmt_service, fwd_service).await?;

    Ok(())
}
