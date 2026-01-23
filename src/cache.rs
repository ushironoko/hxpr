use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;
use xdg::BaseDirectories;

use crate::github::comment::{DiscussionComment, ReviewComment};
use crate::github::{ChangedFile, PullRequest};

#[allow(dead_code)]
pub const DEFAULT_TTL_SECS: u64 = 300; // 5分

/// Sanitize repository name to prevent path traversal attacks.
/// Only allows alphanumeric characters, underscores, hyphens, and single dots (not ".." sequences).
/// Returns a sanitized string with '/' replaced by '_'.
pub fn sanitize_repo_name(repo: &str) -> Result<String> {
    // Check for path traversal patterns
    if repo.contains("..") || repo.starts_with('/') || repo.starts_with('\\') {
        return Err(anyhow::anyhow!(
            "Invalid repository name: contains path traversal pattern"
        ));
    }

    // Replace forward slash with underscore (for owner/repo format)
    let sanitized = repo.replace('/', "_");

    // Validate that the result contains only safe characters
    // Allow: alphanumeric, underscore, hyphen, single dot (for names like "foo.js")
    for c in sanitized.chars() {
        if !c.is_alphanumeric() && c != '_' && c != '-' && c != '.' {
            return Err(anyhow::anyhow!(
                "Invalid repository name: contains invalid character '{}'",
                c
            ));
        }
    }

    // Ensure it doesn't start with a dot (hidden file/directory)
    if sanitized.starts_with('.') {
        return Err(anyhow::anyhow!(
            "Invalid repository name: cannot start with a dot"
        ));
    }

    Ok(sanitized)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub pr: PullRequest,
    pub files: Vec<ChangedFile>,
    pub created_at: u64,
    pub pr_updated_at: String,
}

pub enum CacheResult<T> {
    Hit(T),
    Stale(T),
    Miss,
}

/// キャッシュディレクトリ: ~/.cache/octorus/
pub fn cache_dir() -> PathBuf {
    BaseDirectories::with_prefix("octorus")
        .map(|dirs| dirs.get_cache_home())
        .unwrap_or_else(|_| PathBuf::from(".cache"))
}

/// キャッシュファイルパス: ~/.cache/octorus/{owner}_{repo}_{pr}.json
/// Returns an error if the repository name contains invalid characters or path traversal patterns.
pub fn cache_file_path(repo: &str, pr_number: u32) -> Result<PathBuf> {
    let sanitized = sanitize_repo_name(repo)?;
    Ok(cache_dir().join(format!("{}_{}.json", sanitized, pr_number)))
}

/// キャッシュ読み込み
pub fn read_cache(repo: &str, pr_number: u32, ttl_secs: u64) -> Result<CacheResult<CacheEntry>> {
    let path = cache_file_path(repo, pr_number)?;
    if !path.exists() {
        return Ok(CacheResult::Miss);
    }

    let content = std::fs::read_to_string(&path)?;
    let entry: CacheEntry = serde_json::from_str(&content)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    let age = now.saturating_sub(entry.created_at);

    if age <= ttl_secs {
        Ok(CacheResult::Hit(entry))
    } else {
        Ok(CacheResult::Stale(entry))
    }
}

/// キャッシュ書き込み
pub fn write_cache(
    repo: &str,
    pr_number: u32,
    pr: &PullRequest,
    files: &[ChangedFile],
) -> Result<()> {
    std::fs::create_dir_all(cache_dir())?;

    let entry = CacheEntry {
        pr: pr.clone(),
        files: files.to_vec(),
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs(),
        pr_updated_at: pr.updated_at.clone(),
    };

    let content = serde_json::to_string_pretty(&entry)?;
    std::fs::write(cache_file_path(repo, pr_number)?, content)?;
    Ok(())
}

/// PRキャッシュ削除
#[allow(dead_code)]
pub fn invalidate_cache(repo: &str, pr_number: u32) -> Result<()> {
    let path = cache_file_path(repo, pr_number)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// 全キャッシュ削除（PR + コメント + ディスカッションコメント）
pub fn invalidate_all_cache(repo: &str, pr_number: u32) -> Result<()> {
    // PR cache
    let pr_path = cache_file_path(repo, pr_number)?;
    if pr_path.exists() {
        std::fs::remove_file(pr_path)?;
    }
    // Comment cache
    let comment_path = comment_cache_file_path(repo, pr_number)?;
    if comment_path.exists() {
        std::fs::remove_file(comment_path)?;
    }
    // Discussion comment cache
    let discussion_comment_path = discussion_comment_cache_file_path(repo, pr_number)?;
    if discussion_comment_path.exists() {
        std::fs::remove_file(discussion_comment_path)?;
    }
    Ok(())
}

// ==================== Comment Cache ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentCacheEntry {
    pub comments: Vec<ReviewComment>,
    pub created_at: u64,
}

/// コメントキャッシュファイルパス: ~/.cache/octorus/{owner}_{repo}_{pr}_comments.json
/// Returns an error if the repository name contains invalid characters or path traversal patterns.
pub fn comment_cache_file_path(repo: &str, pr_number: u32) -> Result<PathBuf> {
    let sanitized = sanitize_repo_name(repo)?;
    Ok(cache_dir().join(format!("{}_{}_comments.json", sanitized, pr_number)))
}

/// コメントキャッシュ読み込み
pub fn read_comment_cache(
    repo: &str,
    pr_number: u32,
    ttl_secs: u64,
) -> Result<CacheResult<CommentCacheEntry>> {
    let path = comment_cache_file_path(repo, pr_number)?;
    if !path.exists() {
        return Ok(CacheResult::Miss);
    }

    let content = std::fs::read_to_string(&path)?;
    let entry: CommentCacheEntry = serde_json::from_str(&content)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    let age = now.saturating_sub(entry.created_at);

    if age <= ttl_secs {
        Ok(CacheResult::Hit(entry))
    } else {
        Ok(CacheResult::Stale(entry))
    }
}

/// コメントキャッシュ書き込み
pub fn write_comment_cache(repo: &str, pr_number: u32, comments: &[ReviewComment]) -> Result<()> {
    std::fs::create_dir_all(cache_dir())?;

    let entry = CommentCacheEntry {
        comments: comments.to_vec(),
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs(),
    };

    let content = serde_json::to_string_pretty(&entry)?;
    std::fs::write(comment_cache_file_path(repo, pr_number)?, content)?;
    Ok(())
}

// ==================== Discussion Comment Cache ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionCommentCacheEntry {
    pub comments: Vec<DiscussionComment>,
    pub created_at: u64,
}

/// ディスカッションコメントキャッシュファイルパス: ~/.cache/octorus/{owner}_{repo}_{pr}_discussion_comments.json
/// Returns an error if the repository name contains invalid characters or path traversal patterns.
pub fn discussion_comment_cache_file_path(repo: &str, pr_number: u32) -> Result<PathBuf> {
    let sanitized = sanitize_repo_name(repo)?;
    Ok(cache_dir().join(format!(
        "{}_{}_discussion_comments.json",
        sanitized, pr_number
    )))
}

/// ディスカッションコメントキャッシュ読み込み
pub fn read_discussion_comment_cache(
    repo: &str,
    pr_number: u32,
    ttl_secs: u64,
) -> Result<CacheResult<DiscussionCommentCacheEntry>> {
    let path = discussion_comment_cache_file_path(repo, pr_number)?;
    if !path.exists() {
        return Ok(CacheResult::Miss);
    }

    let content = std::fs::read_to_string(&path)?;
    let entry: DiscussionCommentCacheEntry = serde_json::from_str(&content)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    let age = now.saturating_sub(entry.created_at);

    if age <= ttl_secs {
        Ok(CacheResult::Hit(entry))
    } else {
        Ok(CacheResult::Stale(entry))
    }
}

/// ディスカッションコメントキャッシュ書き込み
pub fn write_discussion_comment_cache(
    repo: &str,
    pr_number: u32,
    comments: &[DiscussionComment],
) -> Result<()> {
    std::fs::create_dir_all(cache_dir())?;

    let entry = DiscussionCommentCacheEntry {
        comments: comments.to_vec(),
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs(),
    };

    let content = serde_json::to_string_pretty(&entry)?;
    std::fs::write(
        discussion_comment_cache_file_path(repo, pr_number)?,
        content,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_repo_name_valid() {
        // Standard owner/repo format
        assert_eq!(
            sanitize_repo_name("owner/repo").unwrap(),
            "owner_repo".to_string()
        );

        // Repo name with hyphens
        assert_eq!(
            sanitize_repo_name("my-org/my-repo").unwrap(),
            "my-org_my-repo".to_string()
        );

        // Repo name with dots (e.g., config files or versioned repos)
        assert_eq!(
            sanitize_repo_name("owner/repo.js").unwrap(),
            "owner_repo.js".to_string()
        );

        // Repo name with underscores
        assert_eq!(
            sanitize_repo_name("my_org/my_repo").unwrap(),
            "my_org_my_repo".to_string()
        );

        // Alphanumeric only
        assert_eq!(
            sanitize_repo_name("owner123/repo456").unwrap(),
            "owner123_repo456".to_string()
        );
    }

    #[test]
    fn test_sanitize_repo_name_path_traversal() {
        // Path traversal with ..
        assert!(sanitize_repo_name("..").is_err());
        assert!(sanitize_repo_name("../foo").is_err());
        assert!(sanitize_repo_name("foo/../bar").is_err());
        assert!(sanitize_repo_name("foo/..").is_err());

        // Absolute path attempts
        assert!(sanitize_repo_name("/etc/passwd").is_err());
        assert!(sanitize_repo_name("\\Windows\\System32").is_err());
    }

    #[test]
    fn test_sanitize_repo_name_hidden_files() {
        // Starting with dot (hidden file/directory)
        assert!(sanitize_repo_name(".hidden").is_err());
        assert!(sanitize_repo_name(".config/repo").is_err());

        // Note: .github is a valid org name on GitHub, but our function rejects
        // names starting with dots for security. This is intentional.
    }

    #[test]
    fn test_sanitize_repo_name_invalid_characters() {
        // Space
        assert!(sanitize_repo_name("owner/repo name").is_err());

        // Special characters
        assert!(sanitize_repo_name("owner/repo@123").is_err());
        assert!(sanitize_repo_name("owner/repo#123").is_err());
        assert!(sanitize_repo_name("owner/repo$var").is_err());
        assert!(sanitize_repo_name("owner/repo%20").is_err());
        assert!(sanitize_repo_name("owner/repo&foo").is_err());
        assert!(sanitize_repo_name("owner/repo*").is_err());
        assert!(sanitize_repo_name("owner/repo;cmd").is_err());
        assert!(sanitize_repo_name("owner/repo|pipe").is_err());

        // Backtick (command injection)
        assert!(sanitize_repo_name("owner/repo`cmd`").is_err());

        // Parentheses
        assert!(sanitize_repo_name("owner/repo(1)").is_err());
    }

    #[test]
    fn test_sanitize_repo_name_unicode() {
        // Note: The current implementation uses is_alphanumeric() which accepts
        // Unicode alphanumeric characters. This is intentional to support
        // international repository names on GitHub.
        // Japanese characters are alphanumeric in Unicode
        assert!(sanitize_repo_name("owner/日本語").is_ok());

        // Emoji are not alphanumeric
        assert!(sanitize_repo_name("owner/repo🚀").is_err());

        // Fullwidth dot/period (U+FF0E) is not alphanumeric
        assert!(sanitize_repo_name("owner/．．").is_err());
    }

    #[test]
    fn test_sanitize_repo_name_edge_cases() {
        // Empty components (multiple slashes become multiple underscores)
        // This is acceptable as it doesn't pose a security risk
        let result = sanitize_repo_name("owner//repo");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "owner__repo");

        // Single name without slash
        assert_eq!(
            sanitize_repo_name("simple-repo").unwrap(),
            "simple-repo".to_string()
        );
    }
}
