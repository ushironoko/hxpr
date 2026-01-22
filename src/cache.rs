use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;
use xdg::BaseDirectories;

use crate::github::comment::{DiscussionComment, ReviewComment};
use crate::github::{ChangedFile, PullRequest};

#[allow(dead_code)]
pub const DEFAULT_TTL_SECS: u64 = 300; // 5分

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
pub fn cache_file_path(repo: &str, pr_number: u32) -> PathBuf {
    let sanitized = repo.replace('/', "_");
    cache_dir().join(format!("{}_{}.json", sanitized, pr_number))
}

/// キャッシュ読み込み
pub fn read_cache(repo: &str, pr_number: u32, ttl_secs: u64) -> Result<CacheResult<CacheEntry>> {
    let path = cache_file_path(repo, pr_number);
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
    std::fs::write(cache_file_path(repo, pr_number), content)?;
    Ok(())
}

/// PRキャッシュ削除
#[allow(dead_code)]
pub fn invalidate_cache(repo: &str, pr_number: u32) -> Result<()> {
    let path = cache_file_path(repo, pr_number);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// 全キャッシュ削除（PR + コメント + ディスカッションコメント）
pub fn invalidate_all_cache(repo: &str, pr_number: u32) -> Result<()> {
    // PR cache
    let pr_path = cache_file_path(repo, pr_number);
    if pr_path.exists() {
        std::fs::remove_file(pr_path)?;
    }
    // Comment cache
    let comment_path = comment_cache_file_path(repo, pr_number);
    if comment_path.exists() {
        std::fs::remove_file(comment_path)?;
    }
    // Discussion comment cache
    let discussion_comment_path = discussion_comment_cache_file_path(repo, pr_number);
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
pub fn comment_cache_file_path(repo: &str, pr_number: u32) -> PathBuf {
    let sanitized = repo.replace('/', "_");
    cache_dir().join(format!("{}_{}_comments.json", sanitized, pr_number))
}

/// コメントキャッシュ読み込み
pub fn read_comment_cache(
    repo: &str,
    pr_number: u32,
    ttl_secs: u64,
) -> Result<CacheResult<CommentCacheEntry>> {
    let path = comment_cache_file_path(repo, pr_number);
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
    std::fs::write(comment_cache_file_path(repo, pr_number), content)?;
    Ok(())
}

// ==================== Discussion Comment Cache ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscussionCommentCacheEntry {
    pub comments: Vec<DiscussionComment>,
    pub created_at: u64,
}

/// ディスカッションコメントキャッシュファイルパス: ~/.cache/octorus/{owner}_{repo}_{pr}_discussion_comments.json
pub fn discussion_comment_cache_file_path(repo: &str, pr_number: u32) -> PathBuf {
    let sanitized = repo.replace('/', "_");
    cache_dir().join(format!("{}_{}_discussion_comments.json", sanitized, pr_number))
}

/// ディスカッションコメントキャッシュ読み込み
pub fn read_discussion_comment_cache(
    repo: &str,
    pr_number: u32,
    ttl_secs: u64,
) -> Result<CacheResult<DiscussionCommentCacheEntry>> {
    let path = discussion_comment_cache_file_path(repo, pr_number);
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
    std::fs::write(discussion_comment_cache_file_path(repo, pr_number), content)?;
    Ok(())
}
