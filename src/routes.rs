use crate::service::{
    delete_service, get_services, list_local_services, list_remote_services, list_service_peers,
    new_update_service,
};
use actix_web::web;

pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/service")
            .route("", web::get().to(get_services))
            .route("", web::post().to(new_update_service))
            .route("", web::delete().to(delete_service)),
    );
    cfg.service(web::scope("/local").route("", web::get().to(list_local_services)));
    cfg.service(web::scope("/remote").route("", web::get().to(list_remote_services)));
    cfg.service(web::scope("/peers").route("", web::get().to(list_service_peers)));
    cfg.service(web::scope("/report").route("", web::get().to(get_services)));
}
