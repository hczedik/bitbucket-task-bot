use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct QueryParams {
    pub bearer: String,
}

// BITBUCKET TYPES:
// note: this is obviously only a subset of all the fields that Bitbucket sends.
// I only implemented those which I need.

#[derive(Deserialize)]
pub struct PullRequestOpenedEvent {
    #[serde(rename = "pullRequest")]
    pub pull_request: PullRequest,
}

#[derive(Deserialize)]
pub struct PullRequest {
    pub id: i64,
    #[serde(rename = "toRef")]
    pub to_ref: Ref,
    #[serde(rename = "fromRef")]
    pub from_ref: Ref,
    pub links: Links,
}

#[derive(Deserialize)]
pub struct Links {
    #[serde(rename = "self")]
    pub self_link: Vec<Link>,
}

#[derive(Deserialize)]
pub struct Link {
    pub href: String,
}

#[derive(Deserialize)]
pub struct Ref {
    pub id: String,
    pub repository: Repository,
}

#[derive(Deserialize)]
pub struct Repository {
    pub slug: String,
    pub project: Project,
}

#[derive(Deserialize)]
pub struct Project {
    pub key: String,
}

#[derive(Serialize)]
pub struct Comment {
    pub text: String,
}

#[derive(Deserialize)]
pub struct PullRequestCommentResponse {
    pub id: i64,
}
