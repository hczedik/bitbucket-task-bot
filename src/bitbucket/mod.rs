// Author: Hermann Czedik-Eysenberg

pub mod types;
use types::*;

use actix_web::client::Client;
use actix_web::error::ErrorInternalServerError;
use actix_web::http::StatusCode;
use actix_web::Error;
use bytes::Bytes;
use futures::future::Future;
use log::error;

pub struct BitbucketClient {
    http_client: Client,
    rest_api_base_url: String,
}

impl BitbucketClient {
    pub fn new(bearer: String, base_url: String) -> BitbucketClient {
        BitbucketClient {
            http_client: Client::build().bearer_auth(bearer).finish(),
            rest_api_base_url: format!("{}rest/api/1.0/", base_url),
        }
    }

    fn get_repo_base_url(&self, repo: &Repository) -> String {
        format!(
            "{}projects/{}/repos/{}/",
            self.rest_api_base_url, repo.project.key, repo.slug
        )
    }

    pub fn comment_pull_request(
        &self,
        repo: Repository,
        pull_request_id: i64,
        comment_text: String,
    ) -> Box<dyn Future<Item = PullRequestCommentResponse, Error = Error>> {
        let pr_comment_url = format!(
            "{}pull-requests/{}/comments",
            self.get_repo_base_url(&repo),
            pull_request_id
        );

        let future = self
            .http_client
            .post(pr_comment_url)
            .send_json(&Comment { text: comment_text })
            .from_err()
            .and_then(|response| {
                if response.status() == StatusCode::CREATED {
                    Ok(response)
                } else {
                    error!("Task creation response: {:?}", response);
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

    pub fn get_raw_file(
        &self,
        repo: &Repository,
        file_path: &str,
    ) -> Box<dyn Future<Item = Bytes, Error = Error>> {
        let file_url = format!("{}raw/{}", self.get_repo_base_url(&repo), file_path);

        let future = self
            .http_client
            .get(&file_url)
            .send()
            .from_err()
            .and_then(|response| {
                if response.status() == StatusCode::OK {
                    Ok(response)
                } else {
                    error!("Read file response: {:?}", response);
                    Err(ErrorInternalServerError(format!(
                        "Unexpected status code for reading file: {}",
                        response.status()
                    )))
                }
            })
            .and_then(move |mut response| {
                response.body().map_err(move |e| {
                    ErrorInternalServerError(format!("Error reading file: {} - {}", file_url, e))
                })
            });
        Box::new(future)
    }

    pub fn add_task_to_comment(
        &self,
        comment_id: i64,
        task_text: String,
    ) -> Box<dyn Future<Item = &'static str, Error = Error>> {
        let task_url = format!("{}tasks", self.rest_api_base_url);

        let future = self
            .http_client
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
}