use libp2p::gossipsub::MessageId;
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{Swarm ,Multiaddr, gossipsub, identity, swarm::SwarmEvent, PeerId};
use std::error::Error;

use async_std::channel;
use futures::StreamExt;
use crate::state::new_state;
use crate::state::{AppState,set,get,all};
use crate::service::{ ApronService, SharedHandler};

pub async fn new(
    // secret_key_seed: Option<u8>,
) -> Result<Swarm<gossipsub::Gossipsub>, Box<dyn Error>> {
    // Create a public/private key pair, either random or based on a seed.
    // let id_keys = match secret_key_seed {
    //     Some(seed) => {
    //         let mut bytes = [0u8; 32];
    //         bytes[0] = seed;
    //         let secret_key = ed25519::SecretKey::from_bytes(&mut bytes).expect(
    //             "this returns `Err` only if the length is wrong; the length is correct; qed",
    //         );
    //         identity::Keypair::Ed25519(secret_key.into())
    //     }
    //     None => identity::Keypair::generate_ed25519(),
    // };
    // let peer_id = id_keys.public().to_peer_id();

    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());

    println!("Local peer id: {:?}", local_peer_id);

    // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(local_key.clone()).await;
    
    // Create a Gossipsub topic
    let topic = Topic::new("apron-test-net");

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

    // let (out_msg_sender, out_msg_receiver) =  channel::unbounded();

    Ok(
        swarm
    )
}

pub async fn network_event_loop(
    mut swarm: Swarm<gossipsub::Gossipsub>,
    receiver: channel::Receiver<String>,
    data: AppState::<ApronService>,
){
    // Create a Gossipsub topic
    let topic = Topic::new("apron-test-net");
    println!("network_event_loop started");
    swarm.behaviour_mut().subscribe(&topic);

    let mut receiver = receiver.fuse();
    

    loop {
        let share_data = data.clone();
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
                            "[libp2p] receive new message {} from remote peer: {:?}",
                            String::from_utf8_lossy(&message.data),
                            peer_id
                        );
                        // update local http gateway data.
                        let key = String::from_utf8_lossy(&message.data).to_string();
                        let new_service = ApronService {
                            id: String::from_utf8_lossy(&message.data).to_string()
                          };                  
                        set(share_data, key, new_service);
                    },
                    _ => {}
                }
            },
            message = receiver.select_next_some() => {
                println!("[libp2p] publish local new message to remote: {}", message);
                swarm.behaviour_mut().publish(topic.clone(), message.as_bytes());
            }
        }
    }
    // println!("network_event_loop ended");
}