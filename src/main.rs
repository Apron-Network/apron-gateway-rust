use crate::routes::routes;
use crate::service::{ApronService, SharedHandler};
use crate::state::new_state;
use crate::state::{all, get, set, AppState};
use actix_cors::Cors;
use actix_web::{get, post, web, web::Data, App, HttpResponse, HttpServer, Responder};
use futures::prelude::*;

use libp2p::gossipsub::MessageId;
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{
    gossipsub, identity, multiaddr::Protocol, swarm::SwarmEvent, Multiaddr, PeerId, Swarm,
};

use async_std::{io, task};
use std::sync::Mutex;
use uuid::Uuid;
// use futures::channel::{mpsc, oneshot};
use async_std::channel;
use std::thread;

use std::{
    error::Error,
    task::{Context, Poll},
};

use structopt::StructOpt;

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

    let (out_msg_sender, out_msg_receiver) = channel::unbounded();

    let data = new_state::<ApronService>();

    // Spawn away the event loop that will keep the swarm going.
    async_std::task::spawn(network::network_event_loop(
        swarm,
        out_msg_receiver,
        data.clone(),
    ));

    let p2p_handler = Data::new(SharedHandler {
        handler: Mutex::new(out_msg_sender),
    });

    let mgmt_local_peer_id = web::Data::new(peer_id.clone());
    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(data.clone())
            .app_data(p2p_handler.clone())
            .app_data(mgmt_local_peer_id.clone())
            .configure(routes)
    })
    .bind(format!("0.0.0.0:{}", opt.mgmt_port))
    .unwrap()
    .run()
    .await;

    Ok(())
}
