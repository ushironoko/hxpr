use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use xdg::BaseDirectories;

use super::{RallyState, RevieweeOutput, ReviewerOutput};
use crate::cache::sanitize_repo_name;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RallySession {
    pub repo: String,
    pub pr_number: u32,
    pub iteration: u32,
    pub state: RallyState,
    pub started_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RallyHistoryEntry {
    pub iteration: u32,
    pub entry_type: HistoryEntryType,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HistoryEntryType {
    Review(ReviewerOutput),
    Fix(RevieweeOutput),
}

fn rally_dir(repo: &str, pr_number: u32) -> Result<PathBuf> {
    let safe_repo = sanitize_repo_name(repo)?;
    let dir = BaseDirectories::with_prefix("octorus")
        .map(|dirs| {
            dirs.get_cache_home()
                .join("rally")
                .join(format!("{}_{}", safe_repo, pr_number))
        })
        .unwrap_or_else(|_| {
            PathBuf::from(".cache/octorus/rally").join(format!("{}_{}", safe_repo, pr_number))
        });
    Ok(dir)
}

pub fn session_path(repo: &str, pr_number: u32) -> Result<PathBuf> {
    Ok(rally_dir(repo, pr_number)?.join("session.json"))
}

pub fn history_dir(repo: &str, pr_number: u32) -> Result<PathBuf> {
    Ok(rally_dir(repo, pr_number)?.join("history"))
}

// For --resume-rally feature (not yet implemented)
#[allow(dead_code)]
pub fn read_session(repo: &str, pr_number: u32) -> Result<Option<RallySession>> {
    let path = session_path(repo, pr_number)?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).context("Failed to read session file")?;
    let session: RallySession =
        serde_json::from_str(&content).context("Failed to parse session file")?;
    Ok(Some(session))
}

pub fn write_session(session: &RallySession) -> Result<()> {
    let dir = rally_dir(&session.repo, session.pr_number)?;
    fs::create_dir_all(&dir).context("Failed to create rally directory")?;

    let path = session_path(&session.repo, session.pr_number)?;
    let content = serde_json::to_string_pretty(session).context("Failed to serialize session")?;

    // Use tempfile for atomic write
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &content).context("Failed to write temporary session file")?;

    // Attempt rename, clean up temp file on failure
    if let Err(e) = fs::rename(&temp_path, &path) {
        // Best effort cleanup of temp file
        let _ = fs::remove_file(&temp_path);
        return Err(e).context("Failed to rename session file");
    }

    Ok(())
}

pub fn write_history_entry(
    repo: &str,
    pr_number: u32,
    iteration: u32,
    entry: &HistoryEntryType,
) -> Result<()> {
    let dir = history_dir(repo, pr_number)?;
    fs::create_dir_all(&dir).context("Failed to create history directory")?;

    let filename = match entry {
        HistoryEntryType::Review(_) => format!("{:03}_review.json", iteration),
        HistoryEntryType::Fix(_) => format!("{:03}_fix.json", iteration),
    };

    let path = dir.join(filename);
    let history_entry = RallyHistoryEntry {
        iteration,
        entry_type: entry.clone(),
        timestamp: chrono_now(),
    };
    let content = serde_json::to_string_pretty(&history_entry)
        .context("Failed to serialize history entry")?;
    fs::write(&path, content).context("Failed to write history file")?;

    Ok(())
}

// For --resume-rally feature (not yet implemented)
#[allow(dead_code)]
pub fn read_history(repo: &str, pr_number: u32) -> Result<Vec<RallyHistoryEntry>> {
    let dir = history_dir(repo, pr_number)?;
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir).context("Failed to read history directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            let content = fs::read_to_string(&path)?;
            if let Ok(history_entry) = serde_json::from_str::<RallyHistoryEntry>(&content) {
                entries.push(history_entry);
            }
        }
    }

    entries.sort_by_key(|e| e.iteration);
    Ok(entries)
}

// For session cleanup after rally completion
#[allow(dead_code)]
pub fn cleanup_session(repo: &str, pr_number: u32) -> Result<()> {
    let dir = rally_dir(repo, pr_number)?;
    if dir.exists() {
        fs::remove_dir_all(&dir).context("Failed to remove rally directory")?;
    }
    Ok(())
}

fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl RallySession {
    pub fn new(repo: &str, pr_number: u32) -> Self {
        let now = chrono_now();
        Self {
            repo: repo.to_string(),
            pr_number,
            iteration: 0,
            state: RallyState::Initializing,
            started_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn update_state(&mut self, state: RallyState) {
        self.state = state;
        self.updated_at = chrono_now();
    }

    pub fn increment_iteration(&mut self) {
        self.iteration += 1;
        self.updated_at = chrono_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::{
        CommentSeverity, ReviewAction, ReviewComment, RevieweeOutput, RevieweeStatus,
        ReviewerOutput,
    };
    use insta::assert_json_snapshot;

    #[test]
    fn test_rally_session_new() {
        let session = RallySession::new("owner/repo", 42);
        assert_json_snapshot!(session, {
            ".started_at" => "[timestamp]",
            ".updated_at" => "[timestamp]",
        }, @r#"
        {
          "repo": "owner/repo",
          "pr_number": 42,
          "iteration": 0,
          "state": "Initializing",
          "started_at": "[timestamp]",
          "updated_at": "[timestamp]"
        }
        "#);
    }

    #[test]
    fn test_rally_session_update_state() {
        let mut session = RallySession::new("owner/repo", 1);
        let before = session.updated_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.update_state(RallyState::ReviewerReviewing);
        assert_eq!(session.state, RallyState::ReviewerReviewing);
        assert_ne!(
            session.updated_at, before,
            "updated_at should change after update_state"
        );
    }

    #[test]
    fn test_rally_session_increment_iteration() {
        let mut session = RallySession::new("owner/repo", 1);
        assert_eq!(session.iteration, 0);
        let before = session.updated_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.increment_iteration();
        assert_eq!(session.iteration, 1);
        assert_ne!(
            session.updated_at, before,
            "updated_at should change after increment"
        );
        session.increment_iteration();
        assert_eq!(session.iteration, 2);
    }

    #[test]
    fn test_history_entry_review_serialization() {
        let entry = RallyHistoryEntry {
            iteration: 1,
            entry_type: HistoryEntryType::Review(ReviewerOutput {
                action: ReviewAction::RequestChanges,
                summary: "Found issues".to_string(),
                comments: vec![ReviewComment {
                    path: "src/main.rs".to_string(),
                    line: 10,
                    body: "Fix this".to_string(),
                    severity: CommentSeverity::Major,
                }],
                blocking_issues: vec!["Error handling".to_string()],
            }),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        assert_json_snapshot!(entry, @r#"
        {
          "iteration": 1,
          "entry_type": {
            "Review": {
              "action": "request_changes",
              "summary": "Found issues",
              "comments": [
                {
                  "path": "src/main.rs",
                  "line": 10,
                  "body": "Fix this",
                  "severity": "major"
                }
              ],
              "blocking_issues": [
                "Error handling"
              ]
            }
          },
          "timestamp": "2024-01-01T00:00:00Z"
        }
        "#);
    }

    #[test]
    fn test_history_entry_fix_serialization() {
        let entry = RallyHistoryEntry {
            iteration: 1,
            entry_type: HistoryEntryType::Fix(RevieweeOutput {
                status: RevieweeStatus::Completed,
                summary: "Fixed all issues".to_string(),
                files_modified: vec!["src/main.rs".to_string()],
                question: None,
                permission_request: None,
                error_details: None,
            }),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        assert_json_snapshot!(entry, @r#"
        {
          "iteration": 1,
          "entry_type": {
            "Fix": {
              "status": "completed",
              "summary": "Fixed all issues",
              "files_modified": [
                "src/main.rs"
              ]
            }
          },
          "timestamp": "2024-01-01T00:00:00Z"
        }
        "#);
    }
}
