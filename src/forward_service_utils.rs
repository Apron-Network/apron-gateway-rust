use std::collections::HashMap;
use std::error::Error;

use actix_web::{web, HttpRequest};
use awc::ClientRequest;
use log::{info, warn};
use url::Url;

use crate::forward_service_models::ProxyRequestInfo;

pub(crate) fn parse_request(
    query_args: web::Query<HashMap<String, String>>,
    raw_body: web::Bytes,
    req: &HttpRequest,
) -> ProxyRequestInfo {
    let mut req_info = ProxyRequestInfo {
        ver: req.match_info().query("ver").parse().unwrap(),
        user_key: req.match_info().query("user_key").parse().unwrap(),
        req_path: req.match_info().query("req_path").parse().unwrap(),
        http_method: req.method().to_string().to_uppercase(),
        headers: Default::default(),
        query_args: query_args.to_owned(),
        raw_body: raw_body.to_vec(),
        json_data: Default::default(),
        form_data: Default::default(),
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

pub fn send_http_request(
    req_info: ProxyRequestInfo,
    base_url: Option<&str>,
) -> Result<ClientRequest, Box<dyn Error>> {
    let client = actix_web::client::Client::new();

    let real_base = match base_url {
        Some(base) => base,
        None => return Err("No API base passed.".into()),
    };

    let mut service_url = Url::parse(real_base).unwrap();
    println!("Base url: {}", real_base);

    // Fill query args
    for (key, val) in req_info.query_args.iter() {
        service_url.query_pairs_mut().append_pair(key, val);
    }

    let mut client_req = match req_info.http_method.as_str() {
        "GET" => client.get(service_url.as_str()),
        "POST" => client.post(service_url.as_str()),
        "PUT" => client.put(service_url.as_str()),
        "DELETE" => client.delete(service_url.as_str()),
        _ => panic!("Unknown http method: {}", req_info.http_method),
    };

    // Fill headers
    for (key, val) in req_info.headers.iter() {
        client_req = client_req.header(key, val.to_owned());
    }

    Ok(client_req)

    // let resp = client_req.send().await.map_err(|e| {
    //     warn!("Send request error: {:?}", e);
    //     e
    // })?;
    // let mut client_resp = HttpResponse::build(resp.status());
    // Ok(client_resp.streaming(resp))
}
