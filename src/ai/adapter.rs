use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::orchestrator::RallyEvent;

/// Context information passed to agents
#[derive(Debug, Clone)]
pub struct Context {
    pub repo: String,
    pub pr_number: u32,
    pub pr_title: String,
    pub pr_body: Option<String>,
    pub diff: String,
    pub working_dir: Option<String>,
    /// HEAD SHA for inline comment posting
    pub head_sha: String,
    /// Base branch name (e.g., "main", "master") for local diff comparison
    pub base_branch: String,
    /// External tool comments (Copilot, CodeRabbit, etc.)
    pub external_comments: Vec<ExternalComment>,
    /// ローカルモードかどうか（GitHub API 呼び出しをスキップ）
    pub local_mode: bool,
    /// ファイル名 → patch のペア（position 変換用）
    pub file_patches: Vec<(String, String)>,
}

/// Comment from external tools (bots)
#[derive(Debug, Clone)]
pub struct ExternalComment {
    /// Source bot name (e.g., "copilot[bot]", "coderabbitai[bot]")
    pub source: String,
    /// File path (None for general comments)
    pub path: Option<String>,
    /// Line number (None for general comments)
    pub line: Option<u32>,
    /// Comment body
    pub body: String,
}

/// Review action from reviewer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    Approve,
    RequestChanges,
    Comment,
}

/// Comment from reviewer with location and severity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub path: String,
    pub line: u32,
    pub body: String,
    pub severity: CommentSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentSeverity {
    Critical,
    Major,
    Minor,
    Suggestion,
}

/// Output from reviewer agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerOutput {
    pub action: ReviewAction,
    pub summary: String,
    pub comments: Vec<ReviewComment>,
    pub blocking_issues: Vec<String>,
}

/// Status from reviewee agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevieweeStatus {
    Completed,
    NeedsClarification,
    NeedsPermission,
    Error,
}

/// Permission request from reviewee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub action: String,
    pub reason: String,
}

/// Output from reviewee agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevieweeOutput {
    pub status: RevieweeStatus,
    pub summary: String,
    pub files_modified: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_request: Option<PermissionRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_details: Option<String>,
}

/// Trait for agent adapters
///
/// NOTE: async-trait is required because native async fn in traits are not dyn-compatible
/// (cannot be used with Box<dyn Trait>). This is needed for runtime polymorphism between
/// different agent implementations (Claude, Codex, Gemini, etc.).
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// Agent name (claude, codex, gemini, etc.)
    ///
    /// Currently unused but kept for future extensibility (e.g., logging which agent
    /// is running, multi-agent coordination, or user-facing agent identification).
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Set event sender for streaming events
    fn set_event_sender(&mut self, sender: mpsc::Sender<RallyEvent>);

    /// Run as reviewer
    async fn run_reviewer(&mut self, prompt: &str, context: &Context) -> Result<ReviewerOutput>;

    /// Run as reviewee
    async fn run_reviewee(&mut self, prompt: &str, context: &Context) -> Result<RevieweeOutput>;

    /// Continue reviewer session (for clarification answers)
    ///
    /// For Clarification/Permission flow (not yet implemented)
    /// See CLAUDE.md "Known Limitations"
    #[allow(dead_code)]
    async fn continue_reviewer(&mut self, message: &str) -> Result<ReviewerOutput>;

    /// Continue reviewee session (for permission grants or clarification answers)
    ///
    /// For Clarification/Permission flow (not yet implemented)
    /// See CLAUDE.md "Known Limitations"
    #[allow(dead_code)]
    async fn continue_reviewee(&mut self, message: &str) -> Result<RevieweeOutput>;

    /// Add a tool to reviewee's allowed tools dynamically
    ///
    /// Used when user grants permission for a specific action (e.g., "Bash(git push:*)").
    /// This allows the reviewee to execute the permitted action in subsequent calls.
    fn add_reviewee_allowed_tool(&mut self, tool: &str);
}

/// Supported agent types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedAgent {
    Claude,
    Codex,
    // Gemini, // Future
}

impl SupportedAgent {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            // "gemini" => Some(Self::Gemini),
            _ => None,
        }
    }

    // For multi-agent coordination (future extensibility)
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            // Self::Gemini => "gemini",
        }
    }
}
