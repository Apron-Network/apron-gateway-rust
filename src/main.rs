use actix_cors::Cors;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;

use actix::{Addr, Arbiter};
use actix_web::{web, web::Data, App, HttpServer};
use env_logger::{Builder, Env};
use futures::channel::mpsc;
use futures::prelude::*;
use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
use log::{info, error};
use structopt::StructOpt;

use crate::forward_service_actors::ServiceSideWsActor;
// use crate::event_loop::EventLoop;
use crate::forward_service_models::{HttpProxyResponse, ProxyData};
use crate::forward_service_utils::connect_to_ws_service;
use crate::network::Command;
use crate::routes::routes;
use crate::service::{ApronService, SharedHandler};
use crate::state::new_state;

use crate::contract::{call, exec};
use crate::usage_report::UsageReportManager;

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
mod usage_report;

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

    #[structopt(default_value = "/ip4/0.0.0.0/tcp/2145", long)]
    p2p_addr: String,

    #[structopt(default_value = "8080", long)]
    forward_port: i32,

    // Note: the mgmt_addr shouldn't contain http:// or https:// prefix, or there will be error said "failed to lookup address information: Name or service not known"
    #[structopt(default_value = "0.0.0.0:8082", long)]
    mgmt_addr: String,

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

fn init_logger() {
    Builder::from_env(Env::default())
        .format_timestamp_nanos()
        .init()
}

#[actix_web::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_logger();
    let opt = Opt::from_args();

    info!("options: {:?}", opt.clone());

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
            Ok(_) => info!("Dialed {:?}", dialing),
            Err(e) => error!("Dial {:?} failed: {:?}", dialing, e),
        };
    };

    swarm
        .listen_on(opt.clone().p2p_addr.parse().unwrap())
        .unwrap();

    let peer_id = swarm.local_peer_id().clone();

    let (mut command_sender, command_receiver) = mpsc::channel(0);
    let (event_sender, mut event_receiver) = mpsc::channel(0);

    let data = new_state::<ApronService>();
    // let service_peer_mapping = new_state::<PeerId>();

// SBP M2 Should this be made configurable too?
    let topic = String::from("apron-test-net");

    // let req_id_client_session_mapping = Data::new(Mutex::new(mpsc::Sender<HttpProxyResponse>));
    let req_id_client_session_mapping = new_state::<mpsc::Sender<HttpProxyResponse>>();

    // Usage report manager
    // The manager will be used in both mgr API and p2p handler, so put it here.
    let usage_report_mgr = UsageReportManager {
        account_reports: HashMap::new(),
    };

    async_std::task::spawn(network::network_event_loop(
        swarm,
        command_receiver,
        event_sender,
        data.clone(),
        req_id_client_session_mapping.clone(),
        opt.clone(),
        usage_report_mgr.clone(),
        data.clone(),
    ));

    let p2p_handler = Data::new(SharedHandler {
        command_sender: Mutex::new(command_sender.clone()),
        // event_reciver: Mutex::new(event_receiver),
    });

    let fwd_service = forward_service::ForwardService {
        service_data: data.clone(),
        port: opt.forward_port,
        p2p_handler: p2p_handler.clone(),
        peer_id,
        req_id_client_session_mapping: req_id_client_session_mapping.clone(),
    }
    .start();

    let mgmt_local_peer_id = web::Data::new(peer_id.clone());
    let mgmt_p2p_handler = p2p_handler.clone();
    let mgmt_usage_report_mgr = web::Data::new(usage_report_mgr);

    let mgmt_service = HttpServer::new(move || {
        // SBP M2 Consider less permissive configurations?
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(data.clone())
            .app_data(mgmt_p2p_handler.clone())
            .app_data(mgmt_local_peer_id.clone())
            .app_data(mgmt_usage_report_mgr.clone())
            .configure(routes)
    })
    .bind(opt.mgmt_addr)?
    .run();

    // Websocket connect loop, which handles those actions:
    // * receives message sent from network event handler,
    // * processes websocket connection
    // * forward message to network handler with data_sender
    Arbiter::spawn(async move {
        let mut req_id_ws_addr_mapping: HashMap<String, Addr<ServiceSideWsActor>> = HashMap::new();
        let (data_sender, mut ws_data_receiver): (
            mpsc::Sender<ProxyData>,
            mpsc::Receiver<ProxyData>,
        ) = mpsc::channel(0);
        let mut remote_peer_id2: Option<PeerId> = None;
        loop {
            futures::select! {
                evt = event_receiver.next() => {
// SBP M2 Consider backpressure handling?
                    info!("main: Receive event: {:?}", evt);
                    match evt {
                        Some(evt) => match evt {
                            network::Event::ProxyRequestToMainLoop {
                                ws_base,
                                info,
                                remote_peer_id,
                            } => {
                                info!(
                                    "ServiceSideGateway: Proxy request received is {:?}",
                                    info.clone().request_id
                                );
                                remote_peer_id2 = Some(remote_peer_id.clone());
                                // TODO: Get ws url from saved service info
                                let addr = connect_to_ws_service(
                                    &ws_base,
                                    remote_peer_id,
                                    info.clone().request_id,
                                    p2p_handler.clone(),
                                    data_sender.clone(),
                                    command_sender.clone(),
                                ).await;
                                req_id_ws_addr_mapping.insert(info.clone().request_id, addr);
                                info!("ServiceSideGateway: InitWsConn: req_id_ws_addr_mapping keys: {:?}, request_id: {:?}", req_id_ws_addr_mapping.keys(), info.clone().request_id);
                            }

                            network::Event::ProxyDataFromClient {
                                data,
                            } => {
                                info!(
                                    "ServiceSideGateway: client side proxy data received {:?}",
                                    data.data.clone()
                                );
                                // TODO: Send data to websocket connection
                                info!("ServiceSideGateway: WsData: req_id_ws_addr_mapping keys: {:?}, request_id: {:?}", req_id_ws_addr_mapping.keys(), data.request_id.clone());
                                let service_addr = req_id_ws_addr_mapping.get(&data.request_id).unwrap();
                                service_addr.do_send(data);
                            }

                            network::Event::ProxyDataFromService {
                                data,
                            } => {
                                info!("ServiceSideGateway: ProxyDataFromService: {:?}", data)
                            }
                        }
                        _ => {
                            info!("Receive event 2: {:#?}", evt);
                        }
                    }
                }
                proxy_data = ws_data_receiver.next() => {
                    match proxy_data {
                        Some(proxy_data) => {
                        info!("Msg received in main loop: {:?}", proxy_data.clone());
                        command_sender.send(Command::SendProxyDataFromService {
                            peer: remote_peer_id2.unwrap(),
                            request_id: proxy_data.clone().request_id,
                            data: bincode::serialize(&proxy_data).unwrap(),
                        }).await.unwrap();
                        }
                        _ => {}
                    }
                }
            }
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
