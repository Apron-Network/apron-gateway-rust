use actix_cors::Cors;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;

use actix::{spawn, Arbiter, ContextFutureSpawner, Response};
use actix_web::body::{Body, ResponseBody};
use actix_web::{web, web::Data, App, HttpResponse, HttpServer};
use async_std::task::block_on;
use awc::http::header::fmt_comma_delimited;
use awc::http::Uri;
use futures::channel::{mpsc, oneshot};
use futures::prelude::*;
use libp2p::gossipsub::Topic;
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
use serde::de::Unexpected::Str;
use structopt::StructOpt;

// use crate::event_loop::EventLoop;
use crate::forward_service_models::HttpProxyResponse;
use crate::forward_service_utils::send_http_request_blocking;
use crate::network::{Event, FileRequest};
use crate::routes::routes;
use crate::service::{ApronService, SharedHandler};
use crate::state::{get, new_state};
use crate::Protocol::Http;

use crate::contract::{call, exec};

// mod event_loop;
mod contract;
mod forward_service;
mod forward_service_actors;
mod forward_service_models;
mod forward_service_utils;
mod fwd_handlers;
mod helpers;
mod network;
mod routes;
mod service;
mod state;

// substrate node rpc
const WS_ENDPOINT: &str = "ws://127.0.0.1:9944";

const MARKET_CONTRACT_ADDR: &str = "5D6CT1gEXqWiRPzGtNiNE74vWJeVfGgjJBhKfoEkK47f5DQH";
const MARKET_ABI_PATH: &str = "./release/services_market.json";

const STAT_CONTRACT_ADDR: &str = "5FUPaZUs2Vk3RypeK2ozeMyTZbQLraoWFKuAwrer1GPoUv8Y";
const STAT_ABI_PATH: &str = "./release/services_statistics.json";

#[derive(Debug, StructOpt, Clone)]
#[structopt(name = "apron gateway")]
pub struct Opt {
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

    #[structopt(default_value = "ws://127.0.0.1:9944", long)]
    ws_endpoint: String,

    #[structopt(default_value = "", long)]
    market_contract_addr: String,

    #[structopt(default_value = "./release/services_market.json", long)]
    market_contract_abi: String,

    #[structopt(default_value = "", long)]
    stat_contract_addr: String,

    #[structopt(default_value = "./release/services_statistics.json", long)]
    stat_contract_abi: String,
}

#[actix_web::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let opt = Opt::from_args();

    let mut swarm = network::new(opt.secret_key_seed).await.unwrap();

    // In case the user provided an address of a peer on the CLI, dial it.
    if let Some(to_dial) = opt.clone().peer {
        let dialing = to_dial.clone();
        let peer_id = match dialing.iter().last() {
            Some(Protocol::P2p(hash)) => PeerId::from_multihash(hash).expect("Valid hash."),
            _ => return Err("Expect peer multiaddr to contain peer ID.".into()),
        };
        swarm
            .behaviour_mut()
            .kademlia
            .add_address(&peer_id, to_dial.clone());
        match swarm.dial_addr(to_dial) {
            Ok(_) => println!("Dialed {:?}", dialing),
            Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
        };
    };

    // Listen on all interfaces and whatever port the OS assigns
    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/tcp/{}", opt.clone().p2p_port)
                .parse()
                .unwrap(),
        )
        .unwrap();

    let peer_id = swarm.local_peer_id().clone();

    let (command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, mut event_receiver) = mpsc::channel(0);

    let data = new_state::<ApronService>();
    // let service_peer_mapping = new_state::<PeerId>();

    let topic = String::from("apron-test-net");

    // let req_id_client_session_mapping = Data::new(Mutex::new(mpsc::Sender<HttpProxyResponse>));
    let req_id_client_session_mapping = new_state::<mpsc::Sender<HttpProxyResponse>>();

    async_std::task::spawn(network::network_event_loop(
        swarm,
        command_receiver,
        event_sender,
        data.clone(),
        req_id_client_session_mapping.clone(),
        opt.clone(),
    ));

    let p2p_handler = Data::new(SharedHandler {
        command_sender: Mutex::new(command_sender),
        // event_reciver: Mutex::new(event_receiver),
    });

    let fwd_service = forward_service::ForwardService {
        port: opt.forward_port,
        p2p_handler: p2p_handler.clone(),
        peer_id,
        req_id_client_session_mapping: req_id_client_session_mapping.clone(),
    }
    .start();

    let mgmt_local_peer_id = web::Data::new(peer_id.clone());

    let mgmt_service = HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(data.clone())
            .app_data(p2p_handler.clone())
            .app_data(mgmt_local_peer_id.clone())
            .configure(routes)
    })
    .bind(format!("0.0.0.0:{}", opt.mgmt_port))?
    .run();

    // Websocket connect loop, which handles those actions:
    // * receives message sent from network event handler,
    // * processes websocket connection
    // * forward message to network handler with data_sender
    Arbiter::spawn(async move {
        match event_receiver.next().await {
            Some(network::Event::ProxyRequestToMainLoop {
                info,
                mut data_sender,
            }) => {
                println!("Proxy request received is {:?}", info.clone().request_id);
                // swarm.behaviour_mut().request_response.send_request()
                data_sender.send(Vec::from("Hello What")).await;
            }
            _ => {}
        }
    });

    future::try_join(mgmt_service, fwd_service).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    // substrate node rpc
    const WS_ENDPOINT: &str = "ws://127.0.0.1:9944";

    const MARKET_CONTRACT_ADDR: &str = "5FwVe1jsNQociUBV17VZs6SxcWbYy8JjULj9KNuY4gvb43uK";
    const MARKET_ABI_PATH: &str = "./release/services_market.json";

    const STAT_CONTRACT_ADDR: &str = "5FrD1UGeUYG9x4t323gQhy5q2zN32i4o4huZyeq7tWqUHqfy";
    const STAT_ABI_PATH: &str = "./release/services_statistics.json";

    #[test]
    fn test_add_service() {
        println!("test_add_service");
        const uuid: &'static str = "1";
        let result = contract::exec(
            WS_ENDPOINT.to_string(),
            MARKET_CONTRACT_ADDR.to_string(),
            MARKET_ABI_PATH.to_string(),
            String::from("add_service"),
            vec![
                format!("\"{}\"", uuid),                                               //uuid
                format!("\"{}\"", "test1"),                                            //name
                format!("\"{}\"", "test1"),                                            //desc
                format!("\"{}\"", "test1"),                                            //logo
                String::from("12345678"),                                              //createTime
                format!("\"{}\"", "test1"), //providerName
                format!("\"{}\"", "5F7Xv7RaJe8BBNULSuRTXWtfn68njP1NqQL5LLf41piRcEJJ"), //providerOwner
                format!("\"{}\"", "test1"),                                            //usage
                format!("\"{}\"", "test1"),                                            //schema
                format!("\"{}\"", "test1"),                                            //pricePlan
                format!("\"{}\"", "test1"),                                            //declaimer
            ],
        );
        println!("result: {:?}", result);
        assert!(result.is_ok());
        match result {
            Ok(r) => {
                println!("exec result: {}", r)
            }
            Err(e) => {
                println!("exec err: {}", e)
            }
        }
    }

    #[test]
    fn test_query() {
        // query query_service_by_index
        let result = contract::call(
            WS_ENDPOINT.to_string(),
            MARKET_CONTRACT_ADDR.to_string(),
            MARKET_ABI_PATH.to_string(),
            String::from("query_service_by_index"),
            vec![String::from("0")],
        );
        println!("result: {:?}", result);
        assert!(result.is_ok());
        match result {
            Ok(r) => {
                println!("call result: {}", r)
            }
            Err(e) => {
                println!("call err: {}", e)
            }
        }
    }

    #[test]
    fn test_submit_usage() {
        const uuid: &'static str = "1";
        let result = contract::exec(
            WS_ENDPOINT.to_string(),
            STAT_CONTRACT_ADDR.to_string(),
            STAT_ABI_PATH.to_string(),
            String::from("submit_usage"),
            vec![
                format!("\"{}\"", uuid),    //service_uuid
                String::from("0"),          //nonce
                format!("\"{}\"", "test1"), //user_key
                String::from("12345678"),   //start_time
                String::from("12345678"),   //end_time
                String::from("12345678"),   //usage
                format!("\"{}\"", "test1"), //price_plan
                String::from("12345678"),   //cost
            ],
        );
        println!("result: {:?}", result);
        assert!(result.is_ok());
        match result {
            Ok(r) => {
                println!("exec result: {}", r)
            }
            Err(e) => {
                println!("exec err: {}", e)
            }
        }
    }
}
