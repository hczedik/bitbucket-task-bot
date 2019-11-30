// Author: Hermann Czedik-Eysenberg

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct WorkflowConfig {
    pub workflow: Vec<Workflow>,
}

#[derive(Deserialize, Debug)]
pub struct Workflow {
    pub merge: Vec<Merge>,
    pub comment: String,
    pub tasks: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct Merge {
    pub from: String,
    pub to: String,
}
