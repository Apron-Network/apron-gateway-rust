use std::collections::HashMap;
use std::error::Error;
use std::iter;

// use async_std::channel;
use async_std::io;
use async_trait::async_trait;
use awc::http::Uri;
use awc::Client;
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use futures::{AsyncWriteExt, StreamExt};
use libp2p::core::upgrade::{read_length_prefixed, write_length_prefixed, ProtocolName};
use libp2p::gossipsub::{GossipsubEvent, IdentTopic as Topic, MessageAuthenticity};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{GetProvidersOk, Kademlia, KademliaEvent, QueryResult};
use libp2p::request_response::{
    ProtocolSupport, RequestResponse, RequestResponseCodec, RequestResponseEvent,
    RequestResponseMessage, ResponseChannel,
};
use libp2p::NetworkBehaviour;
use libp2p::{gossipsub, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
use url::Url;

use crate::forward_service_models::{HttpProxyResponse, ProxyData, ProxyRequestInfo};
use crate::forward_service_utils::send_http_request_blocking;
use crate::service::ApronService;
use crate::state::{delete, get, set, AppState};
use crate::{helpers, Opt};

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

    AddService {
        args: Vec<String>,
    },
}

pub enum Event {
    ProxyRequestToMainLoop {
        info: ProxyRequestInfo,
        data_sender: mpsc::Sender<Vec<u8>>,
    },

    ProxyData {
        data: ProxyData,
    },
}

pub async fn new(secret_key_seed: Option<u8>) -> Result<Swarm<ComposedBehaviour>, Box<dyn Error>> {
    // Create a public/private key pair, either random or based on a seed.
    let (local_key, local_peer_id) = helpers::generate_peer_id_from_seed(secret_key_seed);

    println!("Local peer id: {:?}", local_peer_id);

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

    Ok(swarm)
}

pub async fn network_event_loop(
    mut swarm: Swarm<ComposedBehaviour>,
    mut receiver: mpsc::Receiver<Command>,
    mut event_sender: mpsc::Sender<Event>,
    data: AppState<ApronService>,
    req_id_client_session_mapping: AppState<mpsc::Sender<HttpProxyResponse>>,
    opt: Opt,
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
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint,.. } => {
                        println!("Connected to {} on {}", peer_id, endpoint.get_remote_address());
                        let remote_address = endpoint.get_remote_address();
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, remote_address.clone());
                    }
                    SwarmEvent::ConnectionClosed { peer_id,.. } => {
                        println!("Disconnected from {}", peer_id);
                        swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
                    }
                    SwarmEvent::Behaviour(ComposedEvent::Gossipsub(
                     GossipsubEvent::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    })) => {
                        // update local http gateway data.
                        println!("[libp2p] Recevie new message from remote: {}", peer_id);
                        let value = String::from_utf8_lossy(&message.data).to_string();
                        let new_service: ApronService = serde_json::from_str(&value).unwrap();
                        let key = new_service.id.clone();
                        let service = get(share_data.clone(), key.clone());
                        match service {
                            Some(_service) => {
                                match new_service.is_deleted {
                                    Some(is_deleted) => {
                                        if is_deleted {
                                            println!("[libp2p] Recevie new message to delete service: {}", key.clone());
                                            delete(share_data, key.clone());
                                        }else{
                                            println!("[libp2p] Recevie new message to update service: {}", key.clone());
                                            set(share_data, key.clone(), new_service);
                                        }
                                    }
                                    None => {
                                        println!("[libp2p] Recevie new message to update service: {}", key.clone());
                                        set(share_data, key.clone(), new_service);
                                    }
                                }
                            }
                            None => {
                                println!("[libp2p] Recevie new message to add new service: {}", key.clone());
                                set(share_data, key.clone(), new_service);
                            }
                        }
                    }

                    SwarmEvent::Behaviour(ComposedEvent::RequestResponse(
                        RequestResponseEvent::Message { peer, message },
                    )) => match message {
                        RequestResponseMessage::Request { request, channel, .. } => {
                            println!("[libp2p] receive request message: {:?}, channel: {:?}", request, channel);
                            println!("Request from Peer id {:?}", peer);

                            // get data from request. Currently only for http.
                            let proxy_request_info: ProxyRequestInfo = bincode::deserialize(&request.0).unwrap();
                            println!("ProxyRequestInfo is {:?}", proxy_request_info);

                            let client_side_req_id = proxy_request_info.clone().request_id;

                            if (proxy_request_info.clone().is_websocket) {
                                // Running on service side gateway, after receiving websocket request,
                                // forward the request directly to main loop since the event handler
                                // can't process async tasks well.
                                println!("Forwarding ws request to main loop");
                                let (ws_data_sender, mut ws_data_receiver): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>)= mpsc::channel(0);
                                event_sender.send(Event::ProxyRequestToMainLoop{
                                    info: proxy_request_info.clone(),
                                    data_sender: ws_data_sender,
                                }).await.expect("Event receiver not to be dropped.");

                                match ws_data_receiver.next().await {
                                    Some(data) => {
                                        println!("Proxy data received from main loop is {:?}", data);
                                        let resp = HttpProxyResponse{
                                            request_id: proxy_request_info.request_id,
                                            status_code: 200,
                                            headers: HashMap::new(),
                                            body: data,
                                        };
                                        swarm.behaviour_mut()
                                                .request_response
                                                .send_response(channel, FileResponse(bincode::serialize(&resp).unwrap()));
                                    }
                                    _ => {}
                                }
                            } else {
                                // TODO: Replace this hard coded base to value fetched from service
                                let tmp_base = "http://localhost:8923/anything";
                                let resp = send_http_request_blocking(proxy_request_info.clone(), Some(tmp_base)).unwrap();

                                // Send resp to client side gateway
                                swarm.behaviour_mut()
                                        .request_response
                                        .send_response(channel, FileResponse(bincode::serialize(&resp).unwrap()));
                            }
                        }

                        RequestResponseMessage::Response { request_id, response, } => {
                            println!("[libp2p] receive response message: {:?}, req_id: {:?}", response, request_id);
                            let resp: HttpProxyResponse = bincode::deserialize(&response.0).unwrap();
                            println!(
                                "receive request {:?} Ack from {:?}: {:?}",
                                request_id,
                                peer,
                                resp
                            );

                            println!("================ send response back");

                            let mut sender = get(req_id_client_session_mapping.clone(), resp.clone().request_id).unwrap();
                            sender.send(resp).await.expect("Event receiver not to be dropped.");
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
                        println!("Peer: {:?}, Addresses: {:?}", peer, addresses);
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
                // receive command outside of event loop.
                match command {
                    Some(c) => match c {
                        Command::PublishGossip { data } => {
                            println!("[libp2p] publish local new message to remote: {}", String::from_utf8_lossy(&data));
                            swarm.behaviour_mut().gossipsub.publish(topic.clone(), data);
                        }
                        Command::Dial { peer, peer_addr} => {
                            println!("[libp2p] Dial to peer: {}, peer_addr: {:?}", peer.to_string(), peer_addr);
                        //    swarm.dial_addr(peer_addr.with(Protocol::P2p(peer.into())));
                        }
                        Command::SendRequest { peer, data } => {
                            println!("[libp2p] Send request to peer: {}, data: {}", peer.to_string(), String::from_utf8_lossy(&data));
                            let request_id = swarm.behaviour_mut().request_response.send_request(&peer, FileRequest(data));
                            println!("libp2p Request id of SendRequest command is: {:?}", request_id);
                        }
                        Command::SendResponse { data, channel} => {
                            swarm.behaviour_mut().request_response.send_response( channel, FileResponse(data));
                        }
                        Command::AddService {args} => {
                            if opt.market_contract_addr == "" {
                                println!("[Apron Chain] test for local service, not upload to chain");
                            }else{
                                println!("[Apron Chain] Add service: {:?}", args);
                                crate::contract::add_service(
                                    opt.ws_endpoint.clone(),
                                    opt.market_contract_addr.clone(),
                                    opt.market_contract_abi.clone(),
                                    args,
                                );
                            }

                            // crate::contract::add_service(
                            //     "ws://127.0.0.1:9944".to_string(),
                            //     "5FwVe1jsNQociUBV17VZs6SxcWbYy8JjULj9KNuY4gvb43uK".to_string(),
                            //     "./release/services_market.json".to_string(),
                            //     args,
                            // );
                        }
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
pub struct FileRequest(pub(crate) Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResponse(pub(crate) Vec<u8>);

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
