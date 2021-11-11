use std::collections::HashMap;
use std::io::Error;

use actix_web::{HttpRequest, HttpResponse, Responder, web};
use actix_web::http::StatusCode;
use actix_web_actors::ws;
use log::debug;
use url::Url;

use crate::{forward_service_actors, forward_service_models, forward_service_utils};

async fn send_http_request(req_info: forward_service_models::ProxyRequestInfo) -> Result<forward_service_models::ProxyData, Error> {
    let client = actix_web::client::Client::new();

    // TODO: The base service URL should be replaced with registered service data
    let mut service_url = Url::parse("https://httpbin.org/anything").unwrap();

    // Fill query args
    for (key, val) in req_info.query_args.iter() {
        service_url.query_pairs_mut().append_pair(key, val);
    }

    let mut client_req = match req_info.http_method.as_str() {
        "GET" => client.get(service_url.as_str()),
        "POST" => client.post(service_url.as_str()),
        "PUT" => client.put(service_url.as_str()),
        "DELETE" => client.delete(service_url.as_str()),
        _ => panic!("Unknown http method: {}", req_info.http_method)
    };

    // Fill headers
    for (key, val) in req_info.headers.iter() {
        client_req = client_req.header(key, val.to_owned());
    }

    let mut apron_proxy_data = forward_service_models::ProxyData {
        channel_id: "".to_string(),
        data: vec![],
    };

    let mut response = if req_info.json_data.is_empty() && req_info.form_data.is_empty() && !req_info.raw_body.is_empty() {
        client_req.send_body(req_info.raw_body)
    } else if !req_info.form_data.is_empty() {
        client_req.send_form(&req_info.form_data)
    } else if !req_info.json_data.is_empty() {
        client_req.send_json(&req_info.json_data)
    } else {
        client_req.send()
    }.await.unwrap();

    let resp_body = response.body().limit(20_000_000).await.unwrap().to_vec();

    match response.status() {
        StatusCode::OK => {
            apron_proxy_data.data = resp_body;
            Ok(apron_proxy_data)
        }
        _ => {
            println!("Resp error: {:?}", resp_body);
            Ok(apron_proxy_data)
        }
    }
}

pub(crate) async fn forward_http_proxy_request(query_args: web::Query<HashMap<String, String>>, raw_body: web::Bytes, req: HttpRequest) -> impl Responder {
    // Parse request from client side
    // TODO: Split http and websocket
    let req_info = forward_service_utils::parse_request(query_args, raw_body, &req);

    // For p2p environment, the req_info should be sent to service side gateway via stream
    // TODO: missing fn: send_via_stream

    // Build request sent to forwarded service
    let resp_body = send_http_request(req_info).await.unwrap();

    // TODO: missing fn: pass response back to client side gateway
    // TODO: missing fn: pass response sent from service side gateway, and respond to client

    HttpResponse::Ok().body(resp_body.data)
}

pub(crate) async fn forward_ws_proxy_request(query_args: web::Query<HashMap<String, String>>, req: HttpRequest, stream: web::Payload) -> impl Responder {
    let req_info = forward_service_utils::parse_request(query_args, web::Bytes::new(), &req);
    debug!("ClientSideGateway: Req info: {:?}", req_info);
    // TODO: Change to configured ws server addr
    let resp = ws::start(forward_service_actors::ClientSideWsActor {
        service_uri: "ws://localhost:10000",
        addr: None,
    }, &req, stream);
    println!("Resp: {:?}", resp);
    resp
}
