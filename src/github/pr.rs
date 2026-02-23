use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::client::{gh_api, gh_api_graphql, gh_api_paginate, gh_command, FieldValue};
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
    #[serde(default, rename = "node_id")]
    pub node_id: Option<String>,
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
    #[serde(default)]
    pub viewed: bool,
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

#[derive(Debug, Deserialize)]
struct GraphqlPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphqlPrFileNode {
    path: String,
    #[serde(rename = "viewerViewedState")]
    viewer_viewed_state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphqlPrFilesConnection {
    nodes: Vec<GraphqlPrFileNode>,
    #[serde(rename = "pageInfo")]
    page_info: GraphqlPageInfo,
}

#[derive(Debug, Deserialize)]
struct GraphqlPrNode {
    files: GraphqlPrFilesConnection,
}

#[derive(Debug, Deserialize)]
struct GraphqlFilesViewedStateData {
    node: Option<GraphqlPrNode>,
}

#[derive(Debug, Deserialize)]
struct GraphqlFilesViewedStateResponse {
    data: Option<GraphqlFilesViewedStateData>,
}

pub async fn fetch_files_viewed_state(
    _repo: &str,
    pr_node_id: &str,
) -> Result<HashMap<String, bool>> {
    let query = r#"
query($pullRequestId: ID!, $after: String) {
  node(id: $pullRequestId) {
    ... on PullRequest {
      files(first: 100, after: $after) {
        nodes {
          path
          viewerViewedState
        }
        pageInfo {
          hasNextPage
          endCursor
        }
      }
    }
  }
}
"#;

    let mut viewed_state = HashMap::new();
    let mut after: Option<String> = None;

    loop {
        let mut fields = vec![("pullRequestId", FieldValue::String(pr_node_id))];
        if let Some(cursor) = after.as_deref() {
            fields.push(("after", FieldValue::String(cursor)));
        }

        let response = gh_api_graphql(query, &fields).await?;

        if let Some(errors) = response.get("errors") {
            anyhow::bail!("GitHub GraphQL returned errors: {}", errors);
        }

        let parsed: GraphqlFilesViewedStateResponse = serde_json::from_value(response)
            .context("Failed to parse files viewed-state GraphQL response")?;
        let Some(data) = parsed.data else {
            anyhow::bail!("GitHub GraphQL response missing data");
        };
        let Some(node) = data.node else {
            anyhow::bail!("Pull request node not found for viewed-state query");
        };

        for file in node.files.nodes {
            viewed_state.insert(
                file.path,
                matches!(file.viewer_viewed_state.as_deref(), Some("VIEWED")),
            );
        }

        if node.files.page_info.has_next_page {
            let Some(next_cursor) = node.files.page_info.end_cursor else {
                anyhow::bail!("GitHub GraphQL pageInfo missing endCursor");
            };
            after = Some(next_cursor);
        } else {
            break;
        }
    }

    Ok(viewed_state)
}

pub async fn mark_file_as_viewed(_repo: &str, pr_node_id: &str, path: &str) -> Result<()> {
    let query = r#"
mutation($pullRequestId: ID!, $path: String!) {
  markFileAsViewed(input: { pullRequestId: $pullRequestId, path: $path }) {
    clientMutationId
  }
}
"#;

    let response = gh_api_graphql(
        query,
        &[
            ("pullRequestId", FieldValue::String(pr_node_id)),
            ("path", FieldValue::String(path)),
        ],
    )
    .await?;

    if let Some(errors) = response.get("errors") {
        anyhow::bail!("GitHub GraphQL returned errors: {}", errors);
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_state_filter_as_gh_arg() {
        assert_eq!(PrStateFilter::Open.as_gh_arg(), "open");
        assert_eq!(PrStateFilter::Closed.as_gh_arg(), "closed");
        assert_eq!(PrStateFilter::All.as_gh_arg(), "all");
    }

    #[test]
    fn test_pr_state_filter_display_name() {
        assert_eq!(PrStateFilter::Open.display_name(), "open");
        assert_eq!(PrStateFilter::Closed.display_name(), "closed");
        assert_eq!(PrStateFilter::All.display_name(), "all");
    }

    #[test]
    fn test_pr_state_filter_next_cycle() {
        assert_eq!(PrStateFilter::Open.next(), PrStateFilter::Closed);
        assert_eq!(PrStateFilter::Closed.next(), PrStateFilter::All);
        assert_eq!(PrStateFilter::All.next(), PrStateFilter::Open);
    }
}
