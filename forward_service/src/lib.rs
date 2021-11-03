use std::io::{Error, Result};

use actix_web::{App, HttpServer, middleware, web};

mod handlers;
mod utils;
mod models;
mod actors;

pub struct ForwardService {
    pub port: i32,
}

impl ForwardService {
    pub async fn start(self) -> Result<()> {
        HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .wrap(middleware::NormalizePath::default())
            .route("/v{ver}/{user_key}/{req_path:.*}", web::to(handlers::forward_http_proxy_request))
            .route("/ws/v{ver}/{user_key}/{req_path:.*}", web::get().to(handlers::forward_ws_proxy_request))
        })
            .bind("0.0.0.0:8970")? // TODO: fprintf address with port
            .run();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
