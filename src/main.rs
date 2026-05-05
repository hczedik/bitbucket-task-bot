// Author: Hermann Czedik-Eysenberg

use actix_web::middleware::Logger;
use actix_web::{web, App, Error, HttpServer, Responder};
use env_logger::Env;
use globset::Glob;
use log::{debug, error, info};
use serde::Deserialize;
use serde_json::Value;

mod config;
use config::{Merge, Workflow, WorkflowConfig};

mod bitbucket;
use bitbucket::types::{PullRequestOpenedEvent, Repository};
use bitbucket::BitbucketClient;

#[derive(Deserialize)]
struct QueryParams {
    bearer: String,
}

#[actix_web::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info,actix_web=debug")).init();

    let port = "8084";

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .route("/", web::get().to(index))
            .route("/hook", web::post().to(handle_bitbucket_event))
    })
    .bind(format!("0.0.0.0:{port}"))
    .unwrap()
    .run()
    .await
    .unwrap();
}

async fn index() -> impl Responder {
    "Hi, I'm the Bitbucket Task Bot!"
}

async fn handle_bitbucket_event(
    query: web::Query<QueryParams>,
    payload: String,
) -> Result<&'static str, Error> {
    info!("Received event: {}", payload);

    let json: Value = serde_json::from_str(&payload)?;

    if json["test"].as_bool() == Some(true) {
        Ok("Success")
    } else if json["eventKey"].as_str() == Some("pr:opened") {
        let event: PullRequestOpenedEvent = serde_json::from_value(json)?;
        handle_pr_opened_event(event, &query.bearer).await
    } else {
        Ok("Ignoring unexpected payload")
    }
}

async fn handle_pr_opened_event(
    event: PullRequestOpenedEvent,
    bearer: &str,
) -> Result<&'static str, Error> {
    let pr = event.pull_request;
    let base_url = pr
        .links
        .self_link
        .first()
        .ok_or_else(|| {
            actix_web::error::ErrorInternalServerError("Missing self link in pull request")
        })?
        .href
        .as_str();
    let base_url = get_base_url(base_url)
        .ok_or_else(|| {
            actix_web::error::ErrorInternalServerError(format!(
                "Error reading URL: {base_url}"
            ))
        })?
        .to_string();
    let repo = pr.to_ref.repository;
    let pull_request_id = pr.id;
    let from_ref = get_short_ref_name(&pr.from_ref.id);
    let to_branch = get_short_ref_name(&pr.to_ref.id);

    let client = BitbucketClient::new(base_url, bearer.to_string());

    let config = match load_config_file(&client, &repo).await {
        Err(e) => {
            error!("Error loading config file: {:?}", e);
            client
                .comment_pull_request(
                    &repo,
                    pull_request_id,
                    &format!(
                        "Error reading workflow-tasks.toml configuration file from default branch: {e}"
                    ),
                )
                .await?;
            return Err(e);
        }
        Ok(config) => config,
    };

    debug!("Config: {:?}", config);

    match select_workflow(&config, &from_ref, &to_branch) {
        None => {
            info!("No workflow for merge {from_ref} -> {to_branch}");
            Ok("No workflow")
        }
        Some(workflow) => {
            info!("Triggering workflow for merge {from_ref} -> {to_branch}");
            handle_workflow(&client, &repo, pull_request_id, workflow).await
        }
    }
}

async fn handle_workflow(
    client: &BitbucketClient,
    repo: &Repository,
    pull_request_id: i64,
    workflow: &Workflow,
) -> Result<&'static str, Error> {
    let comment = client
        .comment_pull_request(repo, pull_request_id, &workflow.comment)
        .await?;

    let comment_id = comment.id;
    info!("Commented with id: {comment_id}");

    for task in &workflow.tasks {
        client
            .add_task_to_comment(repo, pull_request_id, comment_id, task)
            .await?;
    }

    Ok("Success")
}

async fn load_config_file(
    client: &BitbucketClient,
    repo: &Repository,
) -> Result<WorkflowConfig, Error> {
    let body = client.get_raw_file(repo, "workflow-tasks.toml").await?;
    toml::from_str::<WorkflowConfig>(&body).map_err(|e| {
        error!("Error reading TOML: {:?}", e);
        actix_web::error::ErrorInternalServerError(format!("Error reading TOML: {e}"))
    })
}

fn select_workflow<'w>(
    config: &'w WorkflowConfig,
    from_ref: &str,
    to_branch: &str,
) -> Option<&'w Workflow> {
    config.workflow.iter().find(|workflow| {
        workflow
            .merge
            .iter()
            .any(|merge| merge_matches(merge, from_ref, to_branch))
    })
}

fn merge_matches(merge: &Merge, from_ref: &str, to_branch: &str) -> bool {
    wildcard_matches(&merge.from, from_ref) && wildcard_matches(&merge.to, to_branch)
}

fn wildcard_matches(wildcard: &str, s: &str) -> bool {
    Glob::new(wildcard).is_ok_and(|g| g.compile_matcher().is_match(s))
}

fn get_base_url(url: &str) -> Option<&str> {
    let after_scheme = url.find("://")? + 3;
    let path_start = after_scheme + url[after_scheme..].find('/')?;
    Some(&url[..path_start + 1])
}

fn get_short_ref_name(long_ref: &str) -> String {
    long_ref
        .strip_prefix("refs/heads/")
        .or_else(|| long_ref.strip_prefix("refs/tags/"))
        .unwrap_or(long_ref)
        .to_string()
}
