use actix_web::{App, HttpServer, middleware, web};
use std::io::Error;

pub struct ForwardService {
    pub port: i32,
}

impl ForwardService {
    pub async fn start(self) {
        HttpServer::new(move || {
            App::new()
                .wrap(middleware::Logger::default())
                .wrap(middleware::NormalizePath::default())
                // .route("/v{ver}/{user_key}/{req_path:.*}", web::to(http_handler::forward_http_proxy_request))
                // .route("/ws/v{ver}/{user_key}/{req_path:.*}", web::get().to(ws_server::forward_ws_proxy_request))
        })
            .bind("0.0.0.0:8970")
            .unwrap()
            .run();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
