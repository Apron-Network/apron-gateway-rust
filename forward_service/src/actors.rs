use actix::*;
use actix::io::SinkWrite;
use actix_codec::Framed;
use actix_web_actors::ws;
use awc::BoxedSocket;
use awc::Client;
use awc::error::WsProtocolError;
use awc::http::Uri;
use awc::ws::{Codec, Frame, Message};
use futures::stream::SplitSink;
use futures::StreamExt;
use log::{debug, error, info};

use crate::models::{ProxyData, TestWsMsg};

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


// Service side actor

pub(crate) struct ClientSideWsActor {
    pub(crate) service_uri: &'static str,
    pub(crate) addr: Option<Addr<ServiceSideWsActor>>,
}

impl Actor for ClientSideWsActor {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("ClientSideGateway: Started to receive message...");

        connect_to_service(self.service_uri, ctx.address())
            .into_actor(self)
            .map(|addr_to_service, forward_ws, ctx| {
                forward_ws.addr = Some(addr_to_service);
            }).spawn(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        Running::Stop
    }
}

// Handler for message sent from client side
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ClientSideWsActor {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        debug!("ClientSideGateway: receive ws msg from client: {:?}", msg);
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
                debug!("Msg: {:?}", text);
                // TODO: Forward message to service ws client
                match &self.addr {
                    None => { error!("Addr not set"); }
                    Some(addr) => {
                        addr.do_send(TestWsMsg(text));
                    }
                }
            }
            _ => (),
        }
    }
}

// Handler for message sent from client side
impl Handler<ProxyData> for ClientSideWsActor {
    type Result = ();

    fn handle(&mut self, msg: ProxyData, ctx: &mut Self::Context) {
        println!("Message: {:?}", msg);
        ctx.binary(msg.data);
    }
}

async fn connect_to_service(service_uri: &str, addr: Addr<ClientSideWsActor>) -> Addr<ServiceSideWsActor> {
    let (resp, framed) = Client::new()
        .ws(service_uri.parse::<Uri>().unwrap())
        .connect()
        .await
        .unwrap();

    println!("Resp: {:?}", resp);

    let (sink, stream) = framed.split();
    ServiceSideWsActor::create(|ctx| {
        ServiceSideWsActor::add_stream(stream, ctx);
        ServiceSideWsActor {
            writer: SinkWrite::new(sink, ctx),
            addr: addr,
        }
    })

    // ws_client_handler::WsClientToService::create(|ctx| {
    //     ws_client_handler::WsClientToService::add_stream(stream, ctx);
    //     ws_client_handler::WsClientToService(SinkWrite::new(sink, ctx))
    // })

    // addr.do_send(ws_client_handler::TestWsMsg("foobar".to_owned()));
}