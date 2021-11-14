use crate::service::{
    create_service, get_services, list_local_services, list_remote_services, list_service_peers,
};
use actix_web::web;

use libp2p::gossipsub::{
    Gossipsub, GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity,
    ValidationMode,
};

use libp2p::{gossipsub, identity, swarm::SwarmEvent, PeerId};

pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/service")
            .route("", web::get().to(get_services))
            .route("", web::post().to(create_service)),
    );
    cfg.service(web::scope("/local").route("", web::get().to(list_local_services)));
    cfg.service(web::scope("/remote").route("", web::get().to(list_remote_services)));
    cfg.service(web::scope("/peers").route("", web::get().to(list_service_peers)));
    cfg.service(web::scope("/report").route("", web::get().to(get_services)));
}
