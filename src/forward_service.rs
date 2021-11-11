use std::collections::HashMap;
use std::io::{Error, Result};

use actix_web::http::StatusCode;
use actix_web::{middleware, web, App, HttpServer};
use actix_web::{HttpRequest, HttpResponse, Responder};
use actix_web_actors::ws;
use libp2p::core::network::Peer;
use log::debug;
use url::Url;

use crate::forward_service_models::ProxyRequestInfo;
use crate::fwd_handlers::{forward_http_proxy_request, forward_ws_proxy_request};
use crate::service::{ApronService, SharedHandler};
use crate::PeerId;

pub struct ForwardService {
    pub port: i32,
    pub p2p_handler: web::Data<SharedHandler>,
    pub local_peer_id: PeerId,
}

impl ForwardService {
    pub async fn start(self) -> Result<()> {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        println!("Forward service listening on: {}", bind_addr);

        HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .wrap(middleware::NormalizePath::default())
                .app_data(self.p2p_handler.clone())
                .app_data(self.local_peer_id.clone())
                .route(
                    "/v{ver}/{user_key}/{req_path:.*}",
                    web::to(forward_http_proxy_request),
                )
                .route(
                    "/ws/v{ver}/{user_key}/{req_path:.*}",
                    web::get().to(forward_ws_proxy_request),
                )
        })
        .bind(bind_addr)?
        .run();

        Ok(())
    }
}
