// Author: Hermann Czedik-Eysenberg

pub mod types;
use types::{Anchor, Comment, PullRequestCommentResponse, Repository, Task};

use actix_web::Error;
use actix_web::error::ErrorInternalServerError;
use log::error;
use reqwest::{Client, StatusCode};
use std::time::Duration;

pub struct BitbucketClient {
    http_client: Client,
    bearer: String,
    rest_api_base_url: String,
}

impl BitbucketClient {
    pub fn new(base_url: String, bearer: String) -> BitbucketClient {
        BitbucketClient {
            http_client: Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to create HTTP client"),
            bearer,
            rest_api_base_url: format!("{base_url}rest/api/1.0/"),
        }
    }

    fn get_repo_base_url(&self, repo: &Repository) -> String {
        let base = &self.rest_api_base_url;
        let project = &repo.project.key;
        let slug = &repo.slug;
        format!("{base}projects/{project}/repos/{slug}/")
    }

    pub async fn comment_pull_request(
        &self,
        repo: &Repository,
        pull_request_id: i64,
        comment_text: &str,
    ) -> Result<PullRequestCommentResponse, Error> {
        let base = self.get_repo_base_url(repo);
        let url = format!("{base}pull-requests/{pull_request_id}/comments");

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.bearer)
            .json(&Comment {
                text: comment_text.to_string(),
            })
            .send()
            .await
            .map_err(|e| ErrorInternalServerError(e.to_string()))?;

        if response.status() != StatusCode::CREATED {
            error!("Comment creation response status: {}", response.status());
            return Err(ErrorInternalServerError(format!(
                "Unexpected status code for comment creation: {}",
                response.status()
            )));
        }

        response
            .json::<PullRequestCommentResponse>()
            .await
            .map_err(|e| {
                ErrorInternalServerError(format!("Error converting response to JSON: {e}"))
            })
    }

    pub async fn get_raw_file(&self, repo: &Repository, file_path: &str) -> Result<String, Error> {
        let base = self.get_repo_base_url(repo);
        let url = format!("{base}raw/{file_path}");

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.bearer)
            .send()
            .await
            .map_err(|e| ErrorInternalServerError(e.to_string()))?;

        if response.status() != StatusCode::OK {
            error!("Read file response status: {}", response.status());
            return Err(ErrorInternalServerError(format!(
                "Unexpected status code for reading file: {}",
                response.status()
            )));
        }

        response
            .text()
            .await
            .map_err(|e| ErrorInternalServerError(format!("Error reading file: {url} - {e}")))
    }

    pub async fn add_task_to_comment(
        &self,
        repo: &Repository,
        pull_request_id: i64,
        comment_id: i64,
        task_text: &str,
    ) -> Result<(), Error> {
        let base = self.get_repo_base_url(repo);
        let url = format!("{base}pull-requests/{pull_request_id}/blocker-comments");

        let response = self
            .http_client
            .post(&url)
            .bearer_auth(&self.bearer)
            .json(&Task {
                parent: Anchor { id: comment_id },
                text: task_text.to_string(),
            })
            .send()
            .await
            .map_err(|e| ErrorInternalServerError(e.to_string()))?;

        if response.status() != StatusCode::CREATED {
            error!("Task creation response status: {}", response.status());
            return Err(ErrorInternalServerError(format!(
                "Unexpected status code for task creation: {}",
                response.status()
            )));
        }

        Ok(())
    }
}
