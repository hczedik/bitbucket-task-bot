use actix_web::client::Client;
use actix_web::error::ErrorInternalServerError;
use actix_web::http::StatusCode;
use actix_web::middleware::Logger;
use actix_web::{web, App, Error, HttpServer, Responder};
use env_logger::Env;
use futures::future;
use futures::future::Future;
use lazy_static::lazy_static;
use log::{error, info};
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
    let project_key = pr.to_ref.repository.project.key;
    let repository_slug = pr.to_ref.repository.slug;

    let rest_api_base_url = format!("{}rest/api/1.0/", base_url);

    let repo_base_url = format!(
        "{}projects/{}/repos/{}/",
        rest_api_base_url, project_key, repository_slug
    );
    let pr_comment_url = format!("{}pull-requests/{}/comments", repo_base_url, pr.id);

    let client = Client::build().bearer_auth(bearer).finish();
    let response = comment_pull_request(&client, &pr_comment_url).and_then(move |comment| {
        info!("Commented with id: {}", comment.id);

        let task_url = format!("{}tasks", rest_api_base_url);

        add_task_to_comment(&client, &task_url, comment.id, "Test task".to_string()).and_then(
            move |_| add_task_to_comment(&client, &task_url, comment.id, "Test task 2".to_string()),
        )
    });

    Box::new(response)
}

fn comment_pull_request(
    client: &Client,
    pr_comment_url: &str,
) -> Box<dyn Future<Item = PullRequestCommentResponse, Error = Error>> {
    let future = client
        .post(pr_comment_url)
        .send_json(&Comment {
            text: "Test comment".to_string(),
        })
        .from_err()
        .and_then(|response| {
            if response.status() == StatusCode::CREATED {
                Ok(response)
            } else {
                info!("Task creation response: {:?}", response);
                Err(ErrorInternalServerError(format!(
                    "Unexpected status code for comment creation: {}",
                    response.status()
                )))
            }
        })
        .and_then(|mut response| {
            response.json::<PullRequestCommentResponse>().map_err(|e| {
                ErrorInternalServerError(format!("Error converting response to JSON: {}", e))
            })
        });
    Box::new(future)
}

fn add_task_to_comment(
    client: &Client,
    task_url: &str,
    comment_id: i64,
    task_text: String,
) -> Box<dyn Future<Item = &'static str, Error = Error>> {
    let future = client
        .post(task_url)
        .send_json(&Task {
            anchor: Anchor {
                id: comment_id,
                anchor_type: "COMMENT".to_string(),
            },
            text: task_text,
        })
        .from_err()
        .and_then(|response| {
            if response.status() == StatusCode::CREATED {
                Ok("Task created")
            } else {
                error!("Task creation response: {:?}", response);
                Err(ErrorInternalServerError(format!(
                    "Unexpected status code for task creation: {}",
                    response.status()
                )))
            }
        });
    Box::new(future)
}

fn get_base_url(url: &str) -> &str {
    URL_HOST_REGEX
        .captures(url)
        .unwrap()
        .get(1)
        .unwrap()
        .as_str()
}
