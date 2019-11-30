// Author: Hermann Czedik-Eysenberg

use actix_web::error::ErrorInternalServerError;
use actix_web::middleware::Logger;
use actix_web::{web, App, Error, HttpServer, Responder};
use bytes::Bytes;
use env_logger::Env;
use futures::future;
use futures::future::Future;
use lazy_static::lazy_static;
use log::{debug, error, info};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::rc::Rc;
use toml;

mod config;
use config::*;

mod bitbucket;
use bitbucket::types::*;
use bitbucket::*;

lazy_static! {
    static ref URL_HOST_REGEX: Regex = Regex::new(r"^(https?://[^/]+/)").unwrap();
}

#[derive(Deserialize)]
pub struct QueryParams {
    pub bearer: String,
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

fn handle_bitbucket_event(
    query: web::Query<QueryParams>,
    payload: String,
) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    info!("Received event: {}", payload);

    let json: Value = match serde_json::from_str(&payload) {
        Err(e) => return Box::new(future::err(e.into())),
        Ok(json) => json,
    };

    if json["test"].as_bool() == Some(true) {
        // Bitbucket connection test
        Box::new(future::ok("Success"))
    } else if json["eventKey"].as_str() == Some("pr:opened") {
        let event: PullRequestOpenedEvent = match serde_json::from_value(json) {
            Err(e) => return Box::new(future::err(e.into())),
            Ok(event) => event,
        };
        handle_pr_opened_event(event, &query.bearer)
    } else {
        Box::new(future::ok("Ignoring unexpected payload"))
    }
}

fn handle_pr_opened_event(
    event: PullRequestOpenedEvent,
    bearer: &str,
) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    let pr = event.pull_request;
    let base_url = get_base_url(&pr.links.self_link[0].href).to_string();
    let repo = pr.to_ref.repository;
    let pull_request_id = pr.id;

    let bitbucket_client = Rc::new(BitbucketClient::new(bearer.to_string(), base_url));

    let future = load_config_file(&bitbucket_client, &repo)
        .and_then(move |config| {
            debug!("Config: {:?}", config);

            // TODO
            let tasks = vec![
                "Task1".to_string(),
                "Task2".to_string(),
                "Task3".to_string(),
                "Task4".to_string(),
            ];

            bitbucket_client
                .comment_pull_request(repo, pull_request_id, "Test comment".to_string())
                .and_then(move |comment| {
                    let comment_id = comment.id;
                    info!("Commented with id: {}", comment_id);
                    add_tasks(bitbucket_client, comment_id, tasks)
                })
        })
        .and_then(|_| Ok("Success"));

    Box::new(future)
}

fn load_config_file(
    client: &BitbucketClient,
    repo: &Repository,
) -> Box<dyn Future<Item = WorkflowConfig, Error = Error>> {
    let future = client
        .get_raw_file(repo, "workflow-tasks.toml")
        .and_then(|body: Bytes| {
            toml::from_slice::<WorkflowConfig>(&body).map_err(|e| {
                // TODO toml reading error should be reported in comment (to every PR)
                error!("Error reading TOML: {:?}", e);
                ErrorInternalServerError(format!("Error reading TOML: {}", e))
            })
        });

    Box::new(future)
}

fn add_tasks(
    client: Rc<BitbucketClient>,
    comment_id: i64,
    tasks: Vec<String>,
) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    let init_future: Box<dyn Future<Item = &'static str, Error = Error>> =
        Box::new(future::ok("init"));

    tasks.iter().fold(init_future, move |future, task| {
        Box::new(future.and_then({
            let client = Rc::clone(&client);
            let task: String = task.clone();
            move |_| client.add_task_to_comment(comment_id, task)
        }))
    })
}

fn get_base_url(url: &str) -> &str {
    URL_HOST_REGEX
        .captures(url)
        .unwrap()
        .get(1)
        .unwrap()
        .as_str()
}
