
use crate::routes::routes;
use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use crate::state::new_state;
use crate::service::ApronService;

mod routes;
mod service;
mod helpers;
mod state;

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    let data = new_state::<ApronService>();

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .configure(routes)
    })
    .bind("0.0.0.0:8888")?
    .run()
    .await
}