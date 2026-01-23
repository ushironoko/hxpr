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

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RallyContext {
    pub pr_title: String,
    pub pr_body: Option<String>,
    pub diff: String,
    pub readme: Option<String>,
    pub claude_md: Option<String>,
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

#[allow(dead_code)]
pub fn context_path(repo: &str, pr_number: u32) -> Result<PathBuf> {
    Ok(rally_dir(repo, pr_number)?.join("context.json"))
}

pub fn history_dir(repo: &str, pr_number: u32) -> Result<PathBuf> {
    Ok(rally_dir(repo, pr_number)?.join("history"))
}

#[allow(dead_code)]
pub fn logs_dir(repo: &str, pr_number: u32) -> Result<PathBuf> {
    Ok(rally_dir(repo, pr_number)?.join("logs"))
}

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
    fs::write(&temp_path, content).context("Failed to write temporary session file")?;
    fs::rename(&temp_path, &path).context("Failed to rename session file")?;

    Ok(())
}

#[allow(dead_code)]
pub fn read_context(repo: &str, pr_number: u32) -> Result<Option<RallyContext>> {
    let path = context_path(repo, pr_number)?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).context("Failed to read context file")?;
    let context: RallyContext =
        serde_json::from_str(&content).context("Failed to parse context file")?;
    Ok(Some(context))
}

#[allow(dead_code)]
pub fn write_context(repo: &str, pr_number: u32, context: &RallyContext) -> Result<()> {
    let dir = rally_dir(repo, pr_number)?;
    fs::create_dir_all(&dir).context("Failed to create rally directory")?;

    let path = context_path(repo, pr_number)?;
    let content = serde_json::to_string_pretty(context).context("Failed to serialize context")?;

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, content).context("Failed to write temporary context file")?;
    fs::rename(&temp_path, &path).context("Failed to rename context file")?;

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

#[allow(dead_code)]
pub fn append_log(repo: &str, pr_number: u32, log_type: &str, message: &str) -> Result<()> {
    let dir = logs_dir(repo, pr_number)?;
    fs::create_dir_all(&dir).context("Failed to create logs directory")?;

    let path = dir.join(format!("{}.log", log_type));
    let timestamp = chrono_now();
    let log_line = format!("[{}] {}\n", timestamp, message);

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(log_line.as_bytes())?;

    Ok(())
}

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
