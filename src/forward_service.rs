use std::io::{Error, Result};

use actix_web::{App, HttpServer, middleware, web};

use crate::forward_service_handlers;
use crate::service::{ApronService, SharedHandler};

pub struct ForwardService {
    pub port: i32,
    pub p2p_handler: web::Data<SharedHandler>,
}

impl ForwardService {
    pub async fn start(self) -> Result<()> {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        println!("Forward service listening on: {}", bind_addr);

        HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .wrap(middleware::NormalizePath::default())
                .route("/v{ver}/{user_key}/{req_path:.*}", web::to(forward_service_handlers::forward_http_proxy_request))
                .route("/ws/v{ver}/{user_key}/{req_path:.*}", web::get().to(forward_service_handlers::forward_ws_proxy_request))
        })
            .bind(bind_addr)? // TODO: fprintf address with port
            .run();

        Ok(())
    }
}
