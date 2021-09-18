use crate::service::{create_service,get_services};
use actix_web::web;

pub fn routes(cfg: &mut web::ServiceConfig) {

    cfg
    .service(
        web::scope("/service")
            .route("", web::get().to(get_services))
            .route("", web::post().to(create_service)),
    );

}