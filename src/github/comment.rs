use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::client::{gh_api_paginate, gh_api_post, FieldValue};
use super::pr::User;

/// ジェネリックなfetch & parse関数（ページネーション対応）
async fn fetch_and_parse<T: DeserializeOwned>(
    endpoint: &str,
    error_context: &'static str,
) -> Result<T> {
    let json = gh_api_paginate(endpoint).await?;
    serde_json::from_value(json).context(error_context)
}

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
    fetch_and_parse(
        &format!("repos/{}/pulls/{}/comments?per_page=100", repo, pr_number),
        "Failed to parse review comments response",
    )
    .await
}

/// ディスカッションコメント（PRの会話タブのコメント）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionComment {
    pub id: u64,
    pub body: String,
    pub user: User,
    pub created_at: String,
}

pub async fn fetch_discussion_comments(
    repo: &str,
    pr_number: u32,
) -> Result<Vec<DiscussionComment>> {
    fetch_and_parse(
        &format!("repos/{}/issues/{}/comments?per_page=100", repo, pr_number),
        "Failed to parse discussion comments response",
    )
    .await
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
    fetch_and_parse(
        &format!("repos/{}/pulls/{}/reviews?per_page=100", repo, pr_number),
        "Failed to parse reviews response",
    )
    .await
}

pub async fn create_review_comment(
    repo: &str,
    pr_number: u32,
    commit_id: &str,
    path: &str,
    position: u32,
    body: &str,
) -> Result<ReviewComment> {
    let endpoint = format!("repos/{}/pulls/{}/comments", repo, pr_number);
    let position_str = position.to_string();
    // NOTE: line/side/subject_type は Pull Request Review の一部としてのみ有効。
    // 単体コメント API (POST /pulls/{n}/comments) では oneOf スキーマに合致せず 422 になる。
    // position パラメータ（patch 内オフセット）を使用する。
    let json = gh_api_post(
        &endpoint,
        &[
            ("body", FieldValue::String(body)),
            ("commit_id", FieldValue::String(commit_id)),
            ("path", FieldValue::String(path)),
            ("position", FieldValue::Raw(&position_str)),
        ],
    )
    .await?;
    serde_json::from_value(json).context("Failed to parse created comment response")
}

/// 複数行レビューコメントを作成する。
///
/// GitHub API の `line`/`start_line`/`side`/`start_side` パラメータを使用。
/// `start_line` < `line` であること。単一行の場合は `create_review_comment` を使用。
#[allow(clippy::too_many_arguments)]
pub async fn create_multiline_review_comment(
    repo: &str,
    pr_number: u32,
    commit_id: &str,
    path: &str,
    start_line: u32,
    end_line: u32,
    side: &str,
    body: &str,
) -> Result<ReviewComment> {
    let endpoint = format!("repos/{}/pulls/{}/comments", repo, pr_number);
    let start_line_str = start_line.to_string();
    let end_line_str = end_line.to_string();
    let json = gh_api_post(
        &endpoint,
        &[
            ("body", FieldValue::String(body)),
            ("commit_id", FieldValue::String(commit_id)),
            ("path", FieldValue::String(path)),
            ("start_line", FieldValue::Raw(&start_line_str)),
            ("line", FieldValue::Raw(&end_line_str)),
            ("start_side", FieldValue::String(side)),
            ("side", FieldValue::String(side)),
            ("subject_type", FieldValue::String("line")),
        ],
    )
    .await?;
    serde_json::from_value(json).context("Failed to parse created multiline comment response")
}

pub async fn create_reply_comment(
    repo: &str,
    pr_number: u32,
    comment_id: u64,
    body: &str,
) -> Result<ReviewComment> {
    let endpoint = format!(
        "repos/{}/pulls/{}/comments/{}/replies",
        repo, pr_number, comment_id
    );
    let json = gh_api_post(&endpoint, &[("body", FieldValue::String(body))]).await?;
    serde_json::from_value(json).context("Failed to parse reply comment response")
}
