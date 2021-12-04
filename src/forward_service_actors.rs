use actix::io::SinkWrite;
use actix::*;
use actix_codec::Framed;
use actix_web::web::Data;
use actix_web_actors::ws;
use awc::error::WsProtocolError;
use awc::http::Uri;
use awc::ws::{Codec, Frame, Message};
use awc::BoxedSocket;
use awc::Client;
use futures::channel::mpsc::Receiver;
use futures::executor::block_on;
use futures::lock::MutexGuard;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use libp2p::PeerId;
use log::{debug, error, info};
use serde::de::Unexpected::Str;
use serde_json::Value::String;

use crate::forward_service_models::{ProxyData, ProxyRequestInfo, TestWsMsg};
use crate::mpsc::Sender;
use crate::network::Command;
use crate::{HttpProxyResponse, SharedHandler};

// Service side actor
pub(crate) struct ServiceSideWsActor {
    pub(crate) writer: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    pub(crate) addr: Addr<ClientSideWsActor>,
}

impl Actor for ServiceSideWsActor {
    type Context = Context<Self>;
}

// Handler for receiving message from service side
impl StreamHandler<Result<Frame, WsProtocolError>> for ServiceSideWsActor {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, ctx: &mut Self::Context) {
        info!("Received service side message: {:?}", msg);
        // TODO: Send message back to client ws session
        if let Ok(Frame::Text(txt)) = msg {
            let proxy_msg = ProxyData {
                channel_id: "".to_string(),
                data: txt.to_vec(),
            };
            self.addr.do_send(proxy_msg);
        }

        ()
    }
}

impl Handler<TestWsMsg> for ServiceSideWsActor {
    type Result = ();

    fn handle(&mut self, msg: TestWsMsg, _ctx: &mut Context<Self>) {
        info!("Message sent to service side: {:?}", msg);
        self.writer.write(Message::Text(msg.0));
    }
}

impl actix::io::WriteHandler<WsProtocolError> for ServiceSideWsActor {}

// Client side actor
pub(crate) struct ClientSideWsActor {
    pub(crate) req_info: ProxyRequestInfo,
    pub(crate) remote_peer_id: PeerId,
    pub(crate) p2p_handler: Data<SharedHandler>,
    pub(crate) resp_receiver: Receiver<HttpProxyResponse>,
}

impl Actor for ClientSideWsActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("ClientSideGateway: Started to receive message...");
        let mut command_sender = self.p2p_handler.command_sender.lock().unwrap();

        // Block until connect request sent to ServiceSideGateway
        block_on(command_sender.send(Command::SendRequest {
            peer: self.remote_peer_id,
            data: bincode::serialize(&self.req_info).unwrap(),
        }));

        Arbiter::spawn(async move {
            loop {
                match self.resp_receiver.next().await {
                    Some(HttpProxyResponse {
                        request_id,
                        status_code,
                        headers,
                        body,
                    }) => {
                        println!("Proxy response received is {:?}", body);
                    }
                    _ => {}
                }
            }
        });
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        Running::Stop
    }
}

// Handler for message sent from client side
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSideWsActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        info!("ClientSideGateway: receive ws msg from client: {:?}", msg);
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Binary(bin)) => {
                // TODO: Forward the data to service if it is ProxyData
                // ctx.binary(bin)
            }
            Ok(ws::Message::Text(text)) => {
                // TODO: Build ProxyData and send to client
                // let msg = ProxyData {
                //     channel_id: "".to_string(),
                //     data: Vec::from(text),
                // };
                info!("Msg: {:?}", text);
                ctx.text(text);
                // TODO: Forward message to service ws client
                // match &self.addr {
                //     None => {
                //         error!("Addr not set");
                //     }
                //     Some(addr) => {
                //         addr.do_send(TestWsMsg(text));
                //     }
                // }
            }
            _ => (),
        }
    }
}
