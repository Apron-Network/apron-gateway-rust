use std::collections::HashMap;

use actix_web::{HttpRequest, web};
use log::{info, warn};

use crate::forward_service_models::ProxyRequestInfo;

pub(crate) fn parse_request(query_args: web::Query<HashMap<String, String>>, raw_body: web::Bytes, req: &HttpRequest) -> ProxyRequestInfo {
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
        req_info.headers.insert(header.0.to_string(), header.1.to_str().unwrap().to_string());
    }

    // Parse json / form data
    let content_type = match req.headers().get("content-type") {
        Some(content_type) => content_type.to_str().unwrap(),
        None => ""
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
