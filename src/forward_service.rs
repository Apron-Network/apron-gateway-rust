use std::collections::HashMap;
use std::io::Result;
use std::sync::mpsc::Sender;

use actix_web::{middleware, web, App, HttpServer};

use crate::fwd_handlers::{forward_http_proxy_request, forward_ws_proxy_request};
use crate::service::SharedHandler;
use crate::state::AppState;
use crate::{HttpProxyResponse, HttpResponse, PeerId};

#[derive(Clone)]
pub struct ForwardService {
    pub port: i32,
    pub p2p_handler: web::Data<SharedHandler>,
    pub peer_id: PeerId,
    pub req_id_client_session_mapping: AppState<Sender<HttpProxyResponse>>,
}

impl ForwardService {
    pub async fn start(self) -> Result<()> {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        println!("Forward service listening on: {}", bind_addr);

        let app_data_peer_id = web::Data::new(self.peer_id.clone());

        HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .wrap(middleware::NormalizePath::default())
                .app_data(self.p2p_handler.clone())
                .app_data(app_data_peer_id.clone())
                .app_data(self.req_id_client_session_mapping.clone())
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
