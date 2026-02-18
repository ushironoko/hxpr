use anyhow::{Context, Result};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DetectRepoError {
    #[error("Not a git repository. Use --repo to specify.")]
    NotGitRepo,
    #[error("No GitHub remote found. Use --repo to specify.")]
    NoGitHubRemote,
    #[error("gh CLI error: {0}")]
    GhError(String),
}

/// Detect the repository name from the current directory using `gh repo view`
pub async fn detect_repo() -> std::result::Result<String, DetectRepoError> {
    let result = tokio::task::spawn_blocking(|| {
        let output = Command::new("gh")
            .args([
                "repo",
                "view",
                "--json",
                "nameWithOwner",
                "-q",
                ".nameWithOwner",
            ])
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    let repo = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if repo.is_empty() {
                        Err(DetectRepoError::NoGitHubRemote)
                    } else {
                        Ok(repo)
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if stderr.contains("not a git repository") {
                        Err(DetectRepoError::NotGitRepo)
                    } else if stderr.contains("no git remotes")
                        || stderr.contains("could not determine")
                    {
                        Err(DetectRepoError::NoGitHubRemote)
                    } else {
                        Err(DetectRepoError::GhError(stderr.trim().to_string()))
                    }
                }
            }
            Err(e) => Err(DetectRepoError::GhError(format!(
                "Failed to execute gh CLI: {}",
                e
            ))),
        }
    })
    .await;

    match result {
        Ok(r) => r,
        Err(e) => Err(DetectRepoError::GhError(format!(
            "spawn_blocking task panicked: {}",
            e
        ))),
    }
}

/// Execute gh CLI command and return stdout
/// Uses spawn_blocking to avoid blocking the tokio runtime
pub async fn gh_command(args: &[&str]) -> Result<String> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    tokio::task::spawn_blocking(move || {
        let output = Command::new("gh")
            .args(&args)
            .output()
            .context("Failed to execute gh CLI - is it installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr_trimmed = stderr.trim();
            let stdout_trimmed = stdout.trim();
            // デバッグログには全文出力
            tracing::debug!(
                stderr = %stderr_trimmed,
                stdout = %stdout_trimmed,
                "gh command failed"
            );
            if stdout_trimmed.is_empty() {
                anyhow::bail!("gh command failed: {}", stderr_trimmed);
            } else {
                // TUI 表示用に stdout を char 単位で 200 文字に制限
                let truncated: String = stdout_trimmed.chars().take(200).collect();
                let suffix = if stdout_trimmed.len() > truncated.len() {
                    "..."
                } else {
                    ""
                };
                anyhow::bail!(
                    "gh command failed: {} ({}{})",
                    stderr_trimmed,
                    truncated,
                    suffix
                );
            }
        }

        String::from_utf8(output.stdout).context("gh output contains invalid UTF-8")
    })
    .await
    .context("spawn_blocking task panicked")?
}

/// Execute gh api command with JSON output
pub async fn gh_api(endpoint: &str) -> Result<serde_json::Value> {
    let output = gh_command(&["api", endpoint]).await?;
    serde_json::from_str(&output).context("Failed to parse gh api response as JSON")
}

/// Execute gh api command with automatic pagination for array endpoints.
/// Fetches all pages and merges into a single JSON array.
/// Caller should include `per_page=100` in endpoint if desired.
pub async fn gh_api_paginate(endpoint: &str) -> Result<serde_json::Value> {
    let output = gh_command(&["api", "--paginate", "--slurp", endpoint]).await?;
    let pages: Vec<serde_json::Value> =
        serde_json::from_str(&output).context("Failed to parse gh api paginated response")?;
    flatten_pages(pages)
}

/// Flatten an array of JSON arrays (from --paginate --slurp) into a single array.
/// Returns an error if any page is not a JSON array.
fn flatten_pages(pages: Vec<serde_json::Value>) -> Result<serde_json::Value> {
    let mut result = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        match page {
            serde_json::Value::Array(items) => result.extend(items.iter().cloned()),
            other => anyhow::bail!(
                "Expected JSON array for page {}, got {}",
                i + 1,
                other_type_name(other)
            ),
        }
    }
    Ok(serde_json::Value::Array(result))
}

fn other_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Field type for gh api command
pub enum FieldValue<'a> {
    /// String field (-f)
    String(&'a str),
    /// Raw/typed field (-F) - for integers, booleans, null
    Raw(&'a str),
}

/// Execute gh api with method and fields
pub async fn gh_api_post(
    endpoint: &str,
    fields: &[(&str, FieldValue<'_>)],
) -> Result<serde_json::Value> {
    let mut args = vec![
        "api".to_string(),
        "--method".to_string(),
        "POST".to_string(),
        endpoint.to_string(),
    ];
    for (key, value) in fields {
        match value {
            FieldValue::String(v) => {
                args.push("-f".to_string());
                args.push(format!("{}={}", key, v));
            }
            FieldValue::Raw(v) => {
                args.push("-F".to_string());
                args.push(format!("{}={}", key, v));
            }
        }
    }
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    tracing::debug!(args = ?args_refs, "gh api post");
    let output = gh_command(&args_refs).await?;
    serde_json::from_str(&output).context("Failed to parse gh api response as JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_flatten_pages_single_page() {
        let pages = vec![json!([1, 2, 3])];
        let result = flatten_pages(pages).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn test_flatten_pages_multiple_pages() {
        let pages = vec![json!([1, 2]), json!([3, 4]), json!([5])];
        let result = flatten_pages(pages).unwrap();
        assert_eq!(result, json!([1, 2, 3, 4, 5]));
    }

    #[test]
    fn test_flatten_pages_empty() {
        let pages: Vec<serde_json::Value> = vec![];
        let result = flatten_pages(pages).unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn test_flatten_pages_empty_arrays() {
        let pages = vec![json!([]), json!([]), json!([])];
        let result = flatten_pages(pages).unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn test_flatten_pages_rejects_non_array_object() {
        let pages = vec![json!([1, 2]), json!({"key": "value"})];
        let err = flatten_pages(pages).unwrap_err();
        assert!(err.to_string().contains("Expected JSON array for page 2"));
        assert!(err.to_string().contains("object"));
    }

    #[test]
    fn test_flatten_pages_rejects_non_array_string() {
        let pages = vec![json!("not an array")];
        let err = flatten_pages(pages).unwrap_err();
        assert!(err.to_string().contains("Expected JSON array for page 1"));
        assert!(err.to_string().contains("string"));
    }

    #[test]
    fn test_flatten_pages_rejects_null() {
        let pages = vec![json!([1]), json!(null)];
        let err = flatten_pages(pages).unwrap_err();
        assert!(err.to_string().contains("page 2"));
        assert!(err.to_string().contains("null"));
    }
}
