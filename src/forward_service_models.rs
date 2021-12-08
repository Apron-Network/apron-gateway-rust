use std::collections::HashMap;

use actix_web::web;
use serde::{Deserialize, Serialize};

// TODO: Can some params be changed to Url
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProxyRequestInfo {
    pub(crate) service_id: String,
    pub(crate) request_id: String,
    pub(crate) ver: u16,
    pub(crate) user_key: String,
    pub(crate) req_path: String,
    pub(crate) http_method: String,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) query_args: HashMap<String, String>,
    pub(crate) raw_body: Vec<u8>,
    pub(crate) json_data: HashMap<String, String>,
    pub(crate) form_data: HashMap<String, String>,
    pub(crate) is_websocket: bool,
}

#[derive(actix::Message, Debug, Serialize, Deserialize)]
#[rtype(result = "()")]
pub struct ProxyData {
    pub(crate) request_id: String,
    pub(crate) is_binary: bool,
    pub(crate) data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HttpProxyResponse {
    pub(crate) is_websocket_resp: bool,
    pub(crate) request_id: String,
    pub(crate) status_code: u16,
    pub(crate) headers: HashMap<String, Vec<u8>>,
    pub(crate) body: Vec<u8>,
}
