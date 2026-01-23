use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::client::{gh_api, gh_command};
use crate::app::ReviewAction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u32,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub head: Branch,
    pub base: Branch,
    pub user: User,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    pub filename: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub patch: Option<String>,
}

pub async fn fetch_pr(repo: &str, pr_number: u32) -> Result<PullRequest> {
    let endpoint = format!("repos/{}/pulls/{}", repo, pr_number);
    let json = gh_api(&endpoint).await?;
    serde_json::from_value(json).context("Failed to parse PR response")
}

pub async fn fetch_changed_files(repo: &str, pr_number: u32) -> Result<Vec<ChangedFile>> {
    let endpoint = format!("repos/{}/pulls/{}/files", repo, pr_number);
    let json = gh_api(&endpoint).await?;
    serde_json::from_value(json).context("Failed to parse changed files response")
}

pub async fn submit_review(
    repo: &str,
    pr_number: u32,
    action: ReviewAction,
    body: &str,
) -> Result<()> {
    let action_flag = match action {
        ReviewAction::Approve => "--approve",
        ReviewAction::RequestChanges => "--request-changes",
        ReviewAction::Comment => "--comment",
    };

    gh_command(&[
        "pr",
        "review",
        &pr_number.to_string(),
        action_flag,
        "-b",
        body,
        "-R",
        repo,
    ])
    .await?;

    Ok(())
}

/// Fetch the raw diff for a PR using `gh pr diff`
pub async fn fetch_pr_diff(repo: &str, pr_number: u32) -> Result<String> {
    gh_command(&["pr", "diff", &pr_number.to_string(), "-R", repo]).await
}
