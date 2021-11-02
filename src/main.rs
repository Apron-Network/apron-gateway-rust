extern crate forward_service;

use std::{
    error::Error,
    task::{Context, Poll},
};
use std::sync::Mutex;
use std::thread;

use actix_web::{App, get, HttpResponse, HttpServer, post, Responder, web, web::Data};
use async_std::{io, task};
// use futures::channel::{mpsc, oneshot};
use async_std::channel;
use futures::prelude::*;
use libp2p::{gossipsub, identity, Multiaddr, multiaddr::Protocol, PeerId, Swarm, swarm::SwarmEvent};
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::gossipsub::MessageId;
use structopt::StructOpt;
use uuid::Uuid;

use crate::routes::routes;
use crate::service::{ApronService, SharedHandler};
use crate::state::{all, AppState, get, set};
use crate::state::new_state;


mod routes;
mod service;
mod helpers;
mod state;
mod network;

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
}


#[actix_web::main]
async fn main() {
    let opt = Opt::from_args();

    let mut swarm = network::new().await.unwrap();

    // In case the user provided an address of a peer on the CLI, dial it.
    if let Some(to_dial) = opt.peer {
        let dialing = to_dial.clone();
        match swarm.dial_addr(to_dial) {
            Ok(_) => println!("Dialed {:?}", dialing),
            Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
        };
    };

    // Listen on all interfaces and whatever port the OS assigns
    swarm
        .listen_on(format!("/ip4/0.0.0.0/tcp/{}", opt.p2p_port).parse().unwrap())
        .unwrap();

    let (out_msg_sender, out_msg_receiver) = channel::unbounded();

    let data = new_state::<ApronService>();

    // Spawn away the event loop that will keep the swarm going.
    async_std::task::spawn(network::network_event_loop(swarm, out_msg_receiver, data.clone()));

    let p2p_handler = Data::new(
        SharedHandler {
            handler: Mutex::new(out_msg_sender),
        }
    );

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .app_data(p2p_handler.clone())
            .configure(routes)
    })
    .bind(format!("0.0.0.0:{}", opt.mgmt_port)).unwrap()
    .run()
    .await;
}


