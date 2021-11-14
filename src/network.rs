use std::error::Error;
use std::iter;
use std::str::FromStr;

use async_std::channel;
use async_std::io;
use async_trait::async_trait;
use futures::prelude::*;
use futures::{AsyncWriteExt, StreamExt};
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed, ProtocolName};
use libp2p::gossipsub::MessageId;
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
// <<<<<<< Updated upstream
use crate::helpers;
use libp2p::identity::ed25519;
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{GetProvidersOk, Kademlia, KademliaEvent, QueryId, QueryResult};
// =======
// use libp2p::{gossipsub, identity, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
// use std::error::Error;

// use crate::service::{ApronService, SharedHandler};
// use crate::state::new_state;
// use crate::state::{all, get, set, AppState};
// use async_std::channel;
// use futures::StreamExt;

// use async_trait::async_trait;
// use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed, ProtocolName};
// use libp2p::multiaddr::Protocol;
// >>>>>>> Stashed changes
use libp2p::request_response::{
    ProtocolSupport, RequestId, RequestResponse, RequestResponseCodec, RequestResponseEvent,
    RequestResponseMessage, ResponseChannel,
};
use libp2p::swarm::{ProtocolsHandlerUpgrErr, SwarmBuilder};
use libp2p::NetworkBehaviour;
use libp2p::{gossipsub, identity, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};

use crate::service::{ApronService, SharedHandler};
use crate::state::new_state;
use crate::state::{all, get, set, AppState};

#[derive(NetworkBehaviour)]
#[behaviour(event_process = false, out_event = "ComposedEvent")]
pub struct ComposedBehaviour {
    pub request_response: RequestResponse<DataExchangeCodec>,
    pub gossipsub: gossipsub::Gossipsub,
    pub kademlia: Kademlia<MemoryStore>,
}

#[derive(Debug)]
pub enum ComposedEvent {
    RequestResponse(RequestResponseEvent<FileRequest, FileResponse>),
    Gossipsub(GossipsubEvent),
    Kademlia(KademliaEvent),
}

impl From<RequestResponseEvent<FileRequest, FileResponse>> for ComposedEvent {
    fn from(event: RequestResponseEvent<FileRequest, FileResponse>) -> Self {
        ComposedEvent::RequestResponse(event)
    }
}

impl From<GossipsubEvent> for ComposedEvent {
    fn from(event: GossipsubEvent) -> Self {
        ComposedEvent::Gossipsub(event)
    }
}

impl From<KademliaEvent> for ComposedEvent {
    fn from(event: KademliaEvent) -> Self {
        ComposedEvent::Kademlia(event)
    }
}

#[derive(Debug)]
pub enum Command {
    PublishGossip {
        data: Vec<u8>,
    },
    SendRequest {
        peer: PeerId,
        data: Vec<u8>,
    },
    SendResponse {
        data: Vec<u8>,
        channel: ResponseChannel<FileResponse>,
    },

    Dial {
        peer: PeerId,
        peer_addr: Multiaddr,
    },
}

pub async fn new(secret_key_seed: Option<u8>) -> Result<Swarm<ComposedBehaviour>, Box<dyn Error>> {
    // Create a public/private key pair, either random or based on a seed.
    let (local_key, local_peer_id) = helpers::generate_peer_id_from_seed(secret_key_seed);

    // let local_key = identity::Keypair::generate_ed25519();
    // let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // println!(
    //     "[libp2p] local peer id to bytes {:?}",
    //     local_peer_id.to_bytes()
    // );
    // println!(
    //     "[libp2p] local peer id base 58 encode {}",
    //     local_peer_id.to_base58()
    // );
    // let encoded = local_peer_id.to_string();
    // let decoded = PeerId::from_str(&encoded);
    // println!("[libp2p] local peer id base 58 decode {}", decoded.unwrap());

    // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(local_key.clone()).await;

    // Create a Gossipsub topic
    let topic = Topic::new("apron-test-net");

    // Create a Swarm to manage peers and events
    let swarm = {
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

        let request_response = RequestResponse::new(
            DataExchangeCodec(),
            iter::once((DataExchangeProtocol(), ProtocolSupport::Full)),
            Default::default(),
        );
        let kademlia = Kademlia::new(local_peer_id, MemoryStore::new(local_peer_id));

        // match remote_peer_addr {
        //     Some(remote_peer_addr) => {
        //         kademlia.add_address(&local_peer_id, remote_peer_addr);
        //     }
        //     None => {}
        // }

        // build the swarm
        libp2p::Swarm::new(
            transport.unwrap(),
            ComposedBehaviour {
                request_response,
                gossipsub,
                kademlia,
            },
            local_peer_id,
        )
    };

    // let (command_sender, command_receiver) =  channel::unbounded();

    Ok(swarm)
}

pub async fn network_event_loop(
    mut swarm: Swarm<ComposedBehaviour>,
    receiver: channel::Receiver<Command>,
    data: AppState<ApronService>,
) {
    // Create a Gossipsub topic
    let topic = Topic::new("apron-test-net");
    println!("network_event_loop started");
    swarm.behaviour_mut().gossipsub.subscribe(&topic);

    let mut receiver = receiver.fuse();

    loop {
        let share_data = data.clone();
        // let share_service_peer_mapping = service_peer_mapping.clone();
        futures::select! {
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {}", address);
                    },
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint,.. } => {
                        println!("Connected to {} on {}", peer_id, endpoint.get_remote_address());
                        let remote_address = endpoint.get_remote_address();
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, remote_address.clone());
                    },
                    SwarmEvent::ConnectionClosed { peer_id,.. } => {
                        println!("Disconnected from {}", peer_id);
                        swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
                    },
                    SwarmEvent::Behaviour(ComposedEvent::Gossipsub(
                     GossipsubEvent::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    })) => {
                        // update local http gateway data.
                        let value = String::from_utf8_lossy(&message.data).to_string();
                        let new_service: ApronService = serde_json::from_str(&value).unwrap();
                        let key = new_service.id.clone();
                        set(share_data, key.clone(), new_service);
                        // set(share_service_peer_mapping, key, peer_id);
                    },

                    SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                        RequestResponseEvent::Message { peer, message },
                    )) => match message {
                        RequestResponseMessage::Request {
                             request, channel, ..
                        } => {
                            println!("[libp2p] receive request message: {:?}, channel: {:?}", request, channel);
                            println!("Request from Peer id {:?}", peer);
                            // get data from request. Currently only for http.
                            // the data is String
                            println!("Data is {:?}", String::from_utf8_lossy(&request.0));
                            // @Todo forward message to Service Gateway

                            // The response is sent using another request // Send Ack to remote
                            // Send Ack to remote
                     swarm.behaviour_mut()
                            .request_response
                            .send_response(channel, FileResponse(String::from("ok").into_bytes()));
                        }
                        RequestResponseMessage::Response {
                            request_id,
                            response,
                        } => {
                            println!("[libp2p] receive response message: {:?}, req_id: {:?}", response, request_id);
                            println!(
                                "recevie request {:?} Ack from {:?}: {:?}",
                                request_id,
                                peer,
                                String::from_utf8_lossy(&response.0).to_string()
                            );
                        }
                    }

                    SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                        RequestResponseEvent::OutboundFailure {
                            request_id, error, ..
                        },
                    )) =>{}

                    SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                        RequestResponseEvent::ResponseSent { .. },
                    )) => {}

                    SwarmEvent::Behaviour(ComposedEvent::Kademlia(KademliaEvent::RoutingUpdated {
                        peer, is_new_peer, addresses, bucket_range, old_peer
                    })) => {
                        println!("Addresses: {:?}", addresses);
                        // swarm.behaviour_mut().kademlia.add_address(peer_id, addresses..clone());
                    }

                    SwarmEvent::Behaviour(ComposedEvent::Kademlia(KademliaEvent::OutboundQueryCompleted {
                        id,
                        result: QueryResult::StartProviding(_),
                        ..
                    })) => {
                        // let sender: oneshot::Sender<()> = self
                        //     .pending_start_providing
                        //     .remove(&id)
                        //     .expect("Completed query to be previously pending.");
                        // let _ = sender.send(());
                    }

                    SwarmEvent::Behaviour(ComposedEvent::Kademlia(
                        KademliaEvent::OutboundQueryCompleted {
                            id,
                            result: QueryResult::GetProviders(Ok(GetProvidersOk { providers, .. })),
                            ..
                        }
                    )) => {
                        // let _ = self
                        //     .pending_get_providers
                        //     .remove(&id)
                        //     .expect("Completed query to be previously pending.")
                        //     .send(providers);
                    }

                    _ => {}
                }
            },
            command = receiver.next() =>  {
                // recevie command outside of event loop.
                match command {
                    Some(c) => match c {
                        Command::PublishGossip { data } => {
                            println!("[libp2p] publish local new message to remote: {}", String::from_utf8_lossy(&data));
                            swarm.behaviour_mut().gossipsub.publish(topic.clone(), data);
                        },
                        Command::Dial { peer, peer_addr} => {
                            println!("[libp2p] Dial to peer: {}, peer_addr: {:?}", peer.to_string(), peer_addr);
                        //    swarm.dial_addr(peer_addr.with(Protocol::P2p(peer.into())));
                        },
                        Command::SendRequest { peer, data } => {
                            println!("[libp2p] Send request to peer: {}, data: {}", peer.to_string(), String::from_utf8_lossy(&data));
                            let request_id = swarm.behaviour_mut().request_response.send_request(&peer, FileRequest(data));
                        },
                        Command::SendResponse { data, channel} => {
                            swarm.behaviour_mut().request_response.send_response( channel, FileResponse(data));
                        },
                    }
                    None => {}
                }

            }
        }
    }
    // println!("network_event_loop ended");
}

#[derive(Debug, Clone)]
pub struct DataExchangeProtocol();

#[derive(Clone)]
pub struct DataExchangeCodec();

#[derive(Debug, Clone, PartialEq, Eq)]
// struct FileRequest {
//     schema: String,
//     data: String,
// }
pub struct FileRequest(Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResponse(Vec<u8>);

//    pub struct Payload {
//        pub schema: String,
//        pub data: Vec<u8>,
//    }

impl ProtocolName for DataExchangeProtocol {
    fn protocol_name(&self) -> &[u8] {
        "/file-exchange/1".as_bytes()
    }
}

#[async_trait]
impl RequestResponseCodec for DataExchangeCodec {
    type Protocol = DataExchangeProtocol;
    type Request = FileRequest;
    type Response = FileResponse;

    async fn read_request<T>(
        &mut self,
        _: &DataExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 1_000_000).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        Ok(FileRequest(vec))
        // Ok(FileRequest(String::from_utf8(vec).unwrap()))
    }

    async fn read_response<T>(
        &mut self,
        _: &DataExchangeProtocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let vec = read_length_prefixed(io, 1_000_000).await?;

        if vec.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        Ok(FileResponse(vec))
        //  Ok(FileResponse(String::from_utf8(vec).unwrap()))
    }

    async fn write_request<T>(
        &mut self,
        _: &DataExchangeProtocol,
        io: &mut T,
        FileRequest(data): FileRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &DataExchangeProtocol,
        io: &mut T,
        FileResponse(data): FileResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, data).await?;
        io.close().await?;

        Ok(())
    }
}
