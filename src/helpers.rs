use actix_web::{
    body::Body,
    web::{HttpResponse, Json},
    Error,
};
use libp2p;
use libp2p::identity;
use libp2p::identity::ed25519;
use libp2p::identity::Keypair;
use libp2p::PeerId;
use serde::Serialize;

/// Helper function to reduce boilerplate of an OK/Json response
pub fn respond_json<T>(data: T) -> Result<Json<T>, Error>
where
    T: Serialize,
{
    Ok(Json(data))
}

/// Helper function to reduce boilerplate of an empty OK response
pub fn respond_ok() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body(Body::Empty))
}

pub fn generate_peer_id_from_seed(secret_key_seed: Option<u8>) -> (Keypair, PeerId) {
    // Create a public/private key pair, either random or based on a seed.
    let id_keys = match secret_key_seed {
        Some(seed) => {
            let mut bytes = [0u8; 32];
            bytes[0] = seed;
            let secret_key = ed25519::SecretKey::from_bytes(&mut bytes).expect(
                "this returns `Err` only if the length is wrong; the length is correct; qed",
            );
            identity::Keypair::Ed25519(secret_key.into())
        }
        None => identity::Keypair::generate_ed25519(),
    };
    let peer_id = id_keys.public().into_peer_id();
    (id_keys, peer_id)
}
