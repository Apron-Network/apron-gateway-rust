use actix_web::{body::Body, web::{HttpResponse, Json}, Error};
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

