use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::client::{gh_api, gh_api_post, FieldValue};
use super::pr::User;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: u64,
    pub path: String,
    pub line: Option<u32>,
    pub body: String,
    pub user: User,
    pub created_at: String,
}

pub async fn fetch_review_comments(repo: &str, pr_number: u32) -> Result<Vec<ReviewComment>> {
    let endpoint = format!("repos/{}/pulls/{}/comments", repo, pr_number);
    let json = gh_api(&endpoint).await?;
    serde_json::from_value(json).context("Failed to parse review comments response")
}

/// Issue コメント（PR 全体へのコメント）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueComment {
    pub id: u64,
    pub body: String,
    pub user: User,
    pub created_at: String,
}

pub async fn fetch_issue_comments(repo: &str, pr_number: u32) -> Result<Vec<IssueComment>> {
    let endpoint = format!("repos/{}/issues/{}/comments", repo, pr_number);
    let json = gh_api(&endpoint).await?;
    serde_json::from_value(json).context("Failed to parse issue comments response")
}

/// PR レビュー（全体コメント）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: u64,
    pub body: Option<String>,
    pub state: String,
    pub user: User,
    pub submitted_at: Option<String>,
}

pub async fn fetch_reviews(repo: &str, pr_number: u32) -> Result<Vec<Review>> {
    let endpoint = format!("repos/{}/pulls/{}/reviews", repo, pr_number);
    let json = gh_api(&endpoint).await?;
    serde_json::from_value(json).context("Failed to parse reviews response")
}

pub async fn create_review_comment(
    repo: &str,
    pr_number: u32,
    commit_id: &str,
    path: &str,
    line: u32,
    body: &str,
) -> Result<ReviewComment> {
    let endpoint = format!("repos/{}/pulls/{}/comments", repo, pr_number);
    let line_str = line.to_string();
    let json = gh_api_post(
        &endpoint,
        &[
            ("body", FieldValue::String(body)),
            ("commit_id", FieldValue::String(commit_id)),
            ("path", FieldValue::String(path)),
            ("line", FieldValue::Raw(&line_str)),
            ("side", FieldValue::String("RIGHT")),
        ],
    )
    .await?;
    serde_json::from_value(json).context("Failed to parse created comment response")
}
