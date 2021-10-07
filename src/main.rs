use futures::prelude::*;
use crate::routes::routes;
use actix_web::{get, post, web, App, web::Data, HttpResponse, HttpServer, Responder};
use crate::state::new_state;
use crate::state::{AppState,set,get,all};
use crate::service::{ ApronService, SharedHandler};

use libp2p::gossipsub::MessageId;
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{Swarm ,Multiaddr, gossipsub, identity, swarm::SwarmEvent, PeerId};

use uuid::Uuid;
use std::sync::Mutex;
use async_std::{io, task};
// use futures::channel::{mpsc, oneshot};
use async_std::channel;
use std::thread;

use std::{
    error::Error,
    task::{Context, Poll},
};

mod routes;
mod service;
mod helpers;
mod state;


#[actix_web::main]
async fn main() {
    // init p2p network
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);
    
    // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(local_key.clone()).await;
    
    // Create a Gossipsub topic
    let topic = Topic::new("test-net");
    
    // Create a Swarm to manage peers and events
    let mut swarm = {
        // Set a custom gossipsub
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
            .build()
            .expect("Valid config");
        // build a gossipsub network behaviour
        let mut gossipsub: gossipsub::Gossipsub =
            gossipsub::Gossipsub::new(MessageAuthenticity::Signed(local_key), gossipsub_config)
                .expect("Correct configuration");
        
        // subscribes to our topic
        gossipsub.subscribe(&topic).unwrap();
        
        // build the swarm
        libp2p::Swarm::new(transport.unwrap(), gossipsub, local_peer_id)
    };

    // Reach out to another node if specified
    if let Some(to_dial) = std::env::args().nth(1) {
        let dialing = to_dial.clone();
        match to_dial.parse() {
            Ok(to_dial) => match swarm.dial_addr(to_dial) {
                Ok(_) => println!("Dialed {:?}", dialing),
                Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
            },
            Err(err) => println!("Failed to parse address to dial: {:?}", err),
        }
    }
    
    // Listen on all interfaces and whatever port the OS assigns
    swarm
    .listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
    .unwrap();



    let (out_msg_sender, out_msg_receiver) =  channel::unbounded();

        // Spawn away the event loop that will keep the swarm going.
        async_std::task::spawn(network_event_loop(swarm, out_msg_receiver));

    let p2p_handler = Data::new(
        SharedHandler { 
            handler: Mutex::new(out_msg_sender),
        }
    );

    // let swarm_handler = p2p_handler.clone();
    // let handle = thread::spawn(|| {
    //     let s_handler: Data<SharedHandler> = swarm_handler;
    //     let mut swarm  = s_handler.handler.lock().unwrap();
    //         loop {
    //             match swarm.poll_next_unpin(cx) {
    //                 Poll::Ready(Some(event)) => match event {
    //                     SwarmEvent::Behaviour(GossipsubEvent::Message {
    //                         propagation_source: peer_id,
    //                         message_id: id,
    //                         message,
    //                     }) => println!(
    //                         "Got message: {} with id: {} from peer: {:?}",
    //                         String::from_utf8_lossy(&message.data),
    //                         id,
    //                         peer_id
    //                     ),
    //                     SwarmEvent::NewListenAddr { address, .. } => {
    //                         println!("Listening on {:?}", address);
    //                     }
    //                     _ => {}
    //                 },
    //                 Poll::Ready(None) | Poll::Pending => break,
    //             }
    //         }
    
    //         Poll::Pending
    //     });

    // Reach out to another node if specified
    // if let Some(to_dial) = std::env::args().nth(1) {
    //     let dialing = to_dial.clone();
    //     match to_dial.parse() {
    //         Ok(to_dial) => match swarm.dial_addr(to_dial) {
    //             Ok(_) => println!("Dialed {:?}", dialing),
    //             Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
    //         },
    //         Err(err) => println!("Failed to parse address to dial: {:?}", err),
    //     }
    // }

    // Read full lines from stdin
    // let mut stdin = io::BufReader::new(io::stdin()).lines();

    // Kick it off
    


// let p2p_handler = new_state::<Swarm<gossipsub::Gossipsub>>();
// let p2p_handler = Data<Mutex<Swarm<gossipsub::Gossipsub>>>;
// set(p2p_handler, "swarm".to_string(), swarm);
// let p2p_handler_1 = p2p_handler;


// @ToDo monitor all messages from subscribe channels
// 1, Add new service registed from other p2p node.---> update local cached data
//    send post request to gateway.
// 2, Delete service of disconnected p2p node from local cached data.
//    send delete request to gateway.
if std::env::args().nth(1).unwrap() == "true" {
    let data = new_state::<ApronService>();
    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .app_data(p2p_handler.clone())
            .configure(routes)
    })
    .bind("0.0.0.0:8888").unwrap()
    .run()
    .await;
}else{
    let data = new_state::<ApronService>();
    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            // .app_data(p2p_handler.clone())
            .configure(routes)
    })
    .bind("0.0.0.0:8889").unwrap()
    .run()
    .await;
}
    

}



async fn network_event_loop(
    mut swarm: Swarm<gossipsub::Gossipsub>,
    receiver: channel::Receiver<String>,
){
    // Create a Gossipsub topic
    let topic = Topic::new("test-net");
    println!("network_event_loop started");
    swarm.behaviour_mut().subscribe(&topic);

    let mut receiver = receiver.fuse();

    loop {
        futures::select! {
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {}", address);
                    },
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint,.. } => {
                        println!("Connected to {} on {}", peer_id, endpoint.get_remote_address());
                    },
                    SwarmEvent::ConnectionClosed { peer_id,.. } => {
                        println!("Disconnected from {}", peer_id);
                    },
                    SwarmEvent::Behaviour(GossipsubEvent::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    }) => {
                        println!(
                            "Got message: {} from peer: {:?}",
                            String::from_utf8_lossy(&message.data),
                            peer_id
                        );
                    },
                    _ => {}
                }
            },
            message = receiver.select_next_some() => {
                println!("Got message: {}", message);
                swarm.behaviour_mut().publish(topic.clone(), message.as_bytes());
            }
        }
    }
    println!("network_event_loop ended");
}