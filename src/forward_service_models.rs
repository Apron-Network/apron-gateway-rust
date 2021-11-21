use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(actix::Message, Debug)]
#[rtype(result = "()")]
pub struct TestWsMsg(pub String);

// TODO: Can some params be changed to Url
#[derive(Serialize, Deserialize, Debug)]
pub struct ProxyRequestInfo {
    pub(crate) ver: u16,
    pub(crate) user_key: String,
    pub(crate) req_path: String,
    pub(crate) http_method: String,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) query_args: HashMap<String, String>,
    pub(crate) raw_body: Vec<u8>,
    pub(crate) json_data: HashMap<String, String>,
    pub(crate) form_data: HashMap<String, String>,
}

#[derive(actix::Message, Debug, Serialize, Deserialize)]
#[rtype(result = "()")]
pub struct ProxyData {
    pub(crate) channel_id: String,
    pub(crate) data: Vec<u8>,
}
