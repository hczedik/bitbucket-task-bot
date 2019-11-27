use actix_web::client::Client;
use actix_web::middleware::Logger;
use actix_web::{web, App, Error, HttpServer, Responder};
use env_logger::Env;
use futures::future;
use futures::future::Future;
use lazy_static::lazy_static;
use log::info;
use regex::Regex;
use serde_json::Value;
use std::env;

mod types;
use types::*;

lazy_static! {
    static ref URL_HOST_REGEX: Regex = Regex::new(r"^(https?://[^/]+/)").unwrap();
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

fn index() -> impl Responder {
    "Hi, I'm the Bitbucket Task Bot!"
}

fn handle_bitbucket_event(query: web::Query<QueryParams>, payload: String) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    info!("Received event: {}", payload);

    let json: Value = match serde_json::from_str(&payload) {
        Err(e) => return Box::new(future::err(e.into())),
        Ok(json) => json,
    };

    if json["test"].as_bool() == Some(true) {
        // Bitbucket connection test
        return Box::new(future::ok("Success"));
    } else if json["eventKey"].as_str() == Some("pr:opened") {
        let event: PullRequestOpenedEvent = match serde_json::from_value(json) {
            Err(e) => return Box::new(future::err(e.into())),
            Ok(event) => event,
        };
        return handle_pr_opened_event(event, &query.bearer);
    } else {
        return Box::new(future::ok("Ignoring unexpected payload"));
    }
}

fn handle_pr_opened_event(
    event: PullRequestOpenedEvent,
    bearer: &str
) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    let pr = event.pull_request;
    let base_url = get_base_url(&pr.links.self_link[0].href);
    let project_key = pr.to_ref.repository.project.key;
    let repository_slug = pr.to_ref.repository.slug;

    let repo_base_url = format!(
        "{}rest/api/1.0/projects/{}/repos/{}/",
        base_url, project_key, repository_slug
    );
    let pr_comment_url = format!("{}pull-requests/{}/comments", repo_base_url, pr.id);

    let client = Client::build()
        .bearer_auth(bearer)
        .finish();
    let response = client
        .post(&pr_comment_url)
        .send_json(&Comment {
            text: "Test comment".to_string(),
        })
        .map_err(|e| -> Error { e.into() })
        .and_then(|response| {
            info!("Comment response: {:?}", response);

            Ok("Handled pr:opened event")
        });

    return Box::new(response);
}

fn get_base_url(url: &str) -> &str {
    URL_HOST_REGEX
        .captures(url)
        .unwrap()
        .get(1)
        .unwrap()
        .as_str()
}
