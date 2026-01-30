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
            anyhow::bail!("gh command failed: {}", stderr);
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
    let output = gh_command(&args_refs).await?;
    serde_json::from_str(&output).context("Failed to parse gh api response as JSON")
}
