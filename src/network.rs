use async_std::io;
use futures::prelude::*;
use libp2p::gossipsub::MessageId;
use libp2p::gossipsub::{
    GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
};
use libp2p::{gossipsub, identity, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
use std::error::Error;

use crate::service::{ApronService, SharedHandler};
use crate::state::new_state;
use crate::state::{all, get, set, AppState};
use async_std::channel;
use futures::StreamExt;

use async_trait::async_trait;
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed, ProtocolName};
use libp2p::request_response::{
    ProtocolSupport, RequestId, RequestResponse, RequestResponseCodec, RequestResponseEvent,
    RequestResponseMessage, ResponseChannel,
};
use libp2p::swarm::{ProtocolsHandlerUpgrErr, SwarmBuilder};
use libp2p::NetworkBehaviour;
use std::iter;

#[derive(NetworkBehaviour)]
#[behaviour(event_process = false, out_event = "ComposedEvent")]
pub struct ComposedBehaviour {
    pub request_response: RequestResponse<DataExchangeCodec>,
    pub gossipsub: gossipsub::Gossipsub,
}

#[derive(Debug)]
pub enum ComposedEvent {
    RequestResponse(RequestResponseEvent<FileRequest, FileResponse>),
    Gossipsub(GossipsubEvent),
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
}

pub async fn new(// secret_key_seed: Option<u8>,
) -> Result<Swarm<ComposedBehaviour>, Box<dyn Error>> {
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

        let mut request_response = RequestResponse::new(
            DataExchangeCodec(),
            iter::once((DataExchangeProtocol(), ProtocolSupport::Full)),
            Default::default(),
        );

        // build the swarm
        libp2p::Swarm::new(
            transport.unwrap(),
            ComposedBehaviour {
                request_response,
                gossipsub,
            },
            local_peer_id,
        )
    };

    // let (out_msg_sender, out_msg_receiver) =  channel::unbounded();

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
                    SwarmEvent::Behaviour(ComposedEvent::Gossipsub(
                     GossipsubEvent::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    })) => {
                        println!(
                            "[libp2p] receive new message {} from remote peer: {:?}",
                            String::from_utf8_lossy(&message.data),
                            peer_id
                        );
                        // update local http gateway data.
                        let value = String::from_utf8_lossy(&message.data).to_string();
                        let new_service: ApronService = serde_json::from_str(&value).unwrap();
                        let key = new_service.id.clone();
                        set(share_data, key, new_service);
                    },
                    SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                        RequestResponseEvent::Message { message, .. },
                    )) => match message {
                        RequestResponseMessage::Request {
                            request, channel, ..
                        } => {
                            // recevie request message.
                            // forward message to Service Gateway.

                        }
                        RequestResponseMessage::Response {
                            request_id,
                            response,
                        } => {
                            // receive response message.
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
                        Command::SendRequest { peer, data } => {
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
