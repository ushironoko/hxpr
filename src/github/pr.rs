use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::client::{gh_api, gh_api_paginate, gh_command};
use crate::app::ReviewAction;

/// PR状態フィルタ（型安全）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrStateFilter {
    #[default]
    Open,
    Closed,
    All,
}

impl PrStateFilter {
    pub fn as_gh_arg(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::Open => Self::Closed,
            Self::Closed => Self::All,
            Self::All => Self::Open,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestSummary {
    pub number: u32,
    pub title: String,
    pub state: String,
    pub author: User,
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    pub labels: Vec<Label>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
}

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
    let endpoint = format!("repos/{}/pulls/{}/files?per_page=100", repo, pr_number);
    let json = gh_api_paginate(&endpoint).await?;
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

/// ページネーション結果
pub struct PrListPage {
    pub items: Vec<PullRequestSummary>,
    pub has_more: bool,
}

/// PR一覧取得（limit+1件取得してhas_moreを判定）
pub async fn fetch_pr_list(repo: &str, state: PrStateFilter, limit: u32) -> Result<PrListPage> {
    let output = gh_command(&[
        "pr",
        "list",
        "-R",
        repo,
        "-s",
        state.as_gh_arg(),
        "--json",
        "number,title,state,author,isDraft,labels,updatedAt",
        "--limit",
        &(limit + 1).to_string(),
    ])
    .await?;

    let mut items: Vec<PullRequestSummary> =
        serde_json::from_str(&output).context("Failed to parse PR list response")?;
    let has_more = items.len() > limit as usize;
    items.truncate(limit as usize);

    Ok(PrListPage { items, has_more })
}

/// PR一覧取得（オフセット付き、追加ロード用）
pub async fn fetch_pr_list_with_offset(
    repo: &str,
    state: PrStateFilter,
    offset: u32,
    limit: u32,
) -> Result<PrListPage> {
    // gh pr list doesn't support offset directly, so we fetch offset+limit+1 and skip
    let fetch_count = offset + limit + 1;
    let output = gh_command(&[
        "pr",
        "list",
        "-R",
        repo,
        "-s",
        state.as_gh_arg(),
        "--json",
        "number,title,state,author,isDraft,labels,updatedAt",
        "--limit",
        &fetch_count.to_string(),
    ])
    .await?;

    let all_items: Vec<PullRequestSummary> =
        serde_json::from_str(&output).context("Failed to parse PR list response")?;

    // Check if there are more items beyond what we're returning
    let has_more = all_items.len() > (offset + limit) as usize;

    // Skip the offset items and take limit items
    let items: Vec<PullRequestSummary> = all_items
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    Ok(PrListPage { items, has_more })
}
