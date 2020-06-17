// Author: Hermann Czedik-Eysenberg

use actix_web::error::ErrorInternalServerError;
use actix_web::middleware::Logger;
use actix_web::{web, App, Error, HttpServer, Responder};
use bytes::Bytes;
use env_logger::Env;
use futures::future;
use futures::future::Future;
use globset::Glob;
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

    let port = "8084";

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .route("/", web::get().to(index))
            .route("/hook", web::post().to(handle_bitbucket_event))
    })
    .bind(format!("0.0.0.0:{}", port))
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
    let base_url = match get_base_url(&pr.links.self_link[0].href) {
        None => {
            return Box::new(future::err(ErrorInternalServerError(format!(
                "Error reading URL: {}",
                &pr.links.self_link[0].href
            ))))
        }
        Some(base_url) => base_url.to_string(),
    };
    let repo = pr.to_ref.repository;
    let pull_request_id = pr.id;
    let from_branch = pr.from_ref.id.trim_start_matches("refs/heads/").to_string();
    let to_branch = pr.to_ref.id.trim_start_matches("refs/heads/").to_string();

    let client = Rc::new(BitbucketClient::new(base_url, bearer.to_string()));

    let future = load_config_file(&client, &repo).then(move |result| match result {
        Err(e) => {
            error!("Error loading config file: {:?}", e);

            comment_error(
                client,
                &repo,
                pull_request_id,
                "Error reading workflow-tasks.toml configuration file from default branch",
                e,
            )
        }
        Ok(config) => {
            debug!("Config: {:?}", config);

            match select_workflow(&config, &from_branch, &to_branch) {
                None => {
                    info!("No workflow for merge {} -> {}", from_branch, to_branch);
                    Box::new(future::ok("No workflow"))
                }
                Some(workflow) => {
                    info!(
                        "Triggering workflow for merge {} -> {}",
                        from_branch, to_branch
                    );
                    handle_workflow(client, &repo, pull_request_id, workflow)
                }
            }
        }
    });

    Box::new(future)
}

fn comment_error(
    client: Rc<BitbucketClient>,
    repo: &Repository,
    pull_request_id: i64,
    msg: &str,
    e: Error,
) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    Box::new(
        client
            .comment_pull_request(&repo, pull_request_id, format!("{}: {}", msg, e))
            .and_then(|_| Err(e)),
    )
}

fn select_workflow<'w>(
    config: &'w WorkflowConfig,
    from_branch: &str,
    to_branch: &str,
) -> Option<&'w Workflow> {
    config.workflow.iter().find(|workflow| {
        workflow
            .merge
            .iter()
            .any(|merge| merge_matches(merge, from_branch, to_branch))
    })
}

fn merge_matches(merge: &Merge, from_branch: &str, to_branch: &str) -> bool {
    wildcard_matches(&merge.from, from_branch) && wildcard_matches(&merge.to, to_branch)
}

fn wildcard_matches(wildcard: &str, s: &str) -> bool {
    Glob::new(wildcard)
        .map(|g| g.compile_matcher())
        .map(|m| m.is_match(s))
        .unwrap_or(false)
}

fn handle_workflow(
    client: Rc<BitbucketClient>,
    repo: &Repository,
    pull_request_id: i64,
    workflow: &Workflow,
) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    let tasks = workflow.tasks.clone();
    let future = client
        .comment_pull_request(repo, pull_request_id, workflow.comment.to_string())
        .and_then(move |comment| {
            let comment_id = comment.id;
            info!("Commented with id: {}", comment_id);
            add_tasks(client, comment_id, tasks)
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

fn get_base_url(url: &str) -> Option<&str> {
    URL_HOST_REGEX
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|u| u.as_str())
}
