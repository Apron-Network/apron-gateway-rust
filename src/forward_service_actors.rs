use std::future::Future;
use std::string::String;
use std::sync::Mutex;

use actix::*;
use actix::io::SinkWrite;
use actix_codec::Framed;
use actix_web::web::{Bytes, Data};
use actix_web_actors::ws;
use actix_web_actors::ws::ProtocolError;
use awc::BoxedSocket;
use awc::Client;
use awc::error::WsProtocolError;
use awc::http::Uri;
use awc::ws::{Codec, Frame, Message};
use futures::{FutureExt, SinkExt, StreamExt, TryStreamExt};
use futures::channel::mpsc;
use futures::channel::mpsc::{Receiver, Sender};
use futures::executor::block_on;
use futures::lock::MutexGuard;
use futures::stream::SplitSink;
use libp2p::PeerId;
use log::{debug, error, info};
use reqwest::Proxy;
use serde::de::Unexpected::Str;

use crate::{HttpProxyResponse, SharedHandler};
use crate::forward_service_models::{ProxyData, ProxyRequestInfo};
use crate::network::Command;

// Service side actor, connect to ws service and proxy data between libp2p stream and service
pub(crate) struct ServiceSideWsActor {
    pub(crate) writer: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    pub(crate) client_peer_id: PeerId,
    pub(crate) request_id: String,
    pub(crate) p2p_handler: Data<SharedHandler>,
}

impl Actor for ServiceSideWsActor {
    type Context = Context<Self>;
}

// Handler for receiving message from service side
impl StreamHandler<Result<Frame, WsProtocolError>> for ServiceSideWsActor {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, ctx: &mut Self::Context) {
        info!("Received service side message: {:?}", msg);

        let proxy_data = match msg {
            Ok(Frame::Text(text_msg)) => ProxyData {
                request_id: self.request_id.clone(),
                is_binary: false,
                data: text_msg.to_vec(),
            },
            Ok(Frame::Binary(bin_msg)) => ProxyData {
                request_id: self.request_id.clone(),
                is_binary: true,
                data: bin_msg.to_vec(),
            },
            _ => {
                return;
            }
        };

        info!("ServiceSideGateway: Prepare to send data to client {:?}, data: {:?}", self.client_peer_id, proxy_data);
        let mut command_sender = self.p2p_handler.command_sender.lock().unwrap();
        block_on(command_sender.send(Command::SendProxyData {
            peer: self.client_peer_id,
            data: bincode::serialize(&proxy_data).unwrap(),
        }));
        info!("ServiceSideGateway: Sent data to client {:?}, data: {:?}", self.client_peer_id, proxy_data);
    }
}

impl Handler<ProxyData> for ServiceSideWsActor {
    type Result = ();

    fn handle(&mut self, msg: ProxyData, _ctx: &mut Context<Self>) {
        info!("Message sent to service side: {:?}", msg);
        if msg.is_binary {
            self.writer.write(Message::Binary(Bytes::from(msg.data)));
        } else {
            self.writer
                .write(Message::Text(String::from_utf8(msg.data).unwrap()));
        }
    }
}

impl actix::io::WriteHandler<WsProtocolError> for ServiceSideWsActor {}

// Client side actor, receive message from client side and pass to service side gw with libp2p stream
pub(crate) struct ClientSideWsActor {
    pub(crate) req_info: ProxyRequestInfo,
    pub(crate) service_peer_id: PeerId,
    pub(crate) p2p_handler: Data<SharedHandler>,
}

impl Actor for ClientSideWsActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("ClientSideGateway: Started to receive message...");
        let mut command_sender = self.p2p_handler.command_sender.lock().unwrap();

        // Block until connect request sent to ServiceSideGateway
        block_on(command_sender.send(Command::SendRequest {
            peer: self.service_peer_id,
            data: bincode::serialize(&self.req_info).unwrap(),
        }));
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        Running::Stop
    }
}

// Handler for message sent from client side
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSideWsActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        info!("ClientSideGateway: receive ws msg from client: {:?}", msg);
        let proxy_data = match msg.unwrap() {
            ws::Message::Text(text_msg) => ProxyData {
                request_id: self.req_info.request_id.to_string(),
                is_binary: false,
                data: text_msg.into_bytes().to_vec(),
            },
            ws::Message::Binary(binary_msg) => ProxyData {
                request_id: self.req_info.request_id.to_string(),
                is_binary: true,
                data: binary_msg.to_vec(),
            },
            _ => return,
        };
        let mut command_sender = self.p2p_handler.command_sender.lock().unwrap();
        block_on(command_sender.send(Command::SendProxyData {
            peer: self.service_peer_id,
            data: bincode::serialize(&proxy_data).unwrap(),
        }));
        info!("ClientSideGateway: Sent data to service {:?}, data: {:?}", self.service_peer_id, proxy_data);
    }
}
