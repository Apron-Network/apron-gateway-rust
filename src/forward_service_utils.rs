use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::sync::{Mutex, MutexGuard};

use actix::io::SinkWrite;
use actix::{Actor, Addr, StreamHandler};
use actix_web::web::{head, Data};
use actix_web::{web, HttpRequest};
use actix_web_actors::ws;
use awc::http::{HeaderName, Uri};
use awc::{Client, ClientRequest};
use futures::channel::mpsc;
use futures::channel::mpsc::Sender;
use log::{info, warn};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use reqwest::header::HeaderMap;
use url::Url;

use crate::forward_service_actors::ServiceSideWsActor;
use crate::forward_service_models::ProxyRequestInfo;
use crate::network::Command;
use crate::stream::StreamExt;
use crate::{forward_service_actors, HttpProxyResponse, PeerId, SharedHandler};

pub(crate) fn parse_request(
    query_args: web::Query<HashMap<String, String>>,
    raw_body: web::Bytes,
    req: &HttpRequest,
    is_websocket: bool,
) -> ProxyRequestInfo {
    // Generate unique request_id to receive correct response
    let request_id: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let mut req_info = ProxyRequestInfo {
        service_id: String::from(""), // TODO: Use this id to get service detail in service side gw
        request_id,
        ver: req.match_info().query("ver").parse().unwrap(),
        user_key: req.match_info().query("user_key").parse().unwrap(),
        req_path: req.match_info().query("req_path").parse().unwrap(),
        http_method: req.method().to_string().to_uppercase(),
        headers: Default::default(),
        query_args: query_args.to_owned(),
        raw_body: raw_body.to_vec(),
        json_data: Default::default(),
        form_data: Default::default(),
        is_websocket,
    };

    // TODO: user key should be split into service id and user id.

    // Update header
    for header in req.headers().into_iter() {
        req_info
            .headers
            .insert(header.0.to_string(), header.1.to_str().unwrap().to_string());
    }

    // Parse json / form data
    let content_type = match req.headers().get("content-type") {
        Some(content_type) => content_type.to_str().unwrap(),
        None => "",
    };

    match serde_json::from_slice(&raw_body) {
        Ok(parsed_body) => {
            if content_type.eq("application/json") {
                req_info.json_data = parsed_body;
            } else if content_type.contains("application/x-www-form-urlencoded") {
                req_info.form_data = parsed_body;
            }
        }
        Err(e) => warn!("Parse body to json error: {:?}", e),
    };

    info!("{:?}", req_info);

    return req_info;
}

pub fn send_http_request_blocking(
    req_info: ProxyRequestInfo,
    base_url: Option<&str>,
) -> Result<HttpProxyResponse, Box<dyn Error>> {
    let service_url = match base_url {
        Some(base) => base,
        None => return Err("No API base passed.".into()),
    };

    let client = reqwest::blocking::Client::new();

    let client_req = match req_info.http_method.as_str() {
        "GET" => client.get(service_url),
        "POST" => client.post(service_url),
        "PUT" => client.put(service_url),
        "DELETE" => client.delete(service_url),
        _ => panic!("Unknown http method: {}", req_info.http_method),
    };

    let headers = {
        let mut headers = HeaderMap::new();
        for (key, val) in req_info.headers.iter() {
            headers.insert(HeaderName::try_from(key).unwrap(), val.parse().unwrap());
        }
        headers
    };

    // Fill query args
    let mut query_args = Vec::new();
    for (key, val) in req_info.query_args.iter() {
        query_args.push((key, val));
    }

    let resp = client_req
        .query(&query_args)
        .headers(headers)
        .send()
        .unwrap();

    Ok(HttpProxyResponse {
        is_websocket_resp: false,
        request_id: req_info.request_id,
        status_code: resp.status().as_u16(),
        headers: {
            let mut headers = HashMap::new();
            for (key, value) in resp.headers().iter() {
                headers.insert(key.to_string(), Vec::from(value.to_str().unwrap()));
            }
            headers
        },
        body: Vec::from(resp.text().unwrap()),
    })
}

// Function connects to websocket service, should only be invoked in service side gateway.
pub(super) async fn connect_to_ws_service(
    service_uri: &str,
    remote_peer_id: PeerId,
    request_id: String,
    p2p_handler: web::Data<SharedHandler>,
    data_sender: mpsc::Sender<Vec<u8>>
) -> Addr<ServiceSideWsActor> {
    let (resp, framed) = Client::new()
        .ws(service_uri.parse::<Uri>().unwrap())
        .connect()
        .await
        .unwrap();

    info!("ServiceSideGateway: Resp: {:?}", resp);

    let (sink, stream) = framed.split();
    ServiceSideWsActor::create(|ctx| {
        ServiceSideWsActor::add_stream(stream, ctx);
        ServiceSideWsActor {
            writer: SinkWrite::new(sink, ctx),
            client_peer_id: remote_peer_id,
            request_id,
            p2p_handler,
            data_sender,
        }
    })
}
