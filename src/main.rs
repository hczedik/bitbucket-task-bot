use env_logger::Env;
use log::info;
use std::env;

use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer, Responder, Result};

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "eventKey")]
    event_key: String,
}

fn index() -> impl Responder {
    "Hi, I'm the Bitbucket Task Bot!"
}

fn handle_bitbucket_event(payload: String) -> Result<&'static str> {
    info!("Received event: {}", payload);

    let v: Value = serde_json::from_str(&payload)?;

    if v["test"].as_bool() == Some(true) {
        // Bitbucket connection test
        return Ok("Success");
    }

    let event: Event = serde_json::from_value(v)?;

    // TODO

    Ok("OK")
}

fn main() {
    env::set_var("RUST_BACKTRACE", "1");
    env_logger::from_env(Env::default().default_filter_or("info,actix_web=debug")).init();

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .route("/", web::get().to(index))
            .route("/hook", web::post().to(handle_bitbucket_event))
    })
    .bind("127.0.0.1:8088")
    .unwrap()
    .bind(":8088")
    .unwrap()
    .run()
    .unwrap();
}
