use anyhow::{anyhow, Context as AnyhowContext, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::ai::adapter::{
    AgentAdapter, Context, PermissionRequest, ReviewAction, ReviewComment, RevieweeOutput,
    RevieweeStatus, ReviewerOutput,
};
use crate::ai::orchestrator::RallyEvent;

const REVIEWER_SCHEMA: &str = include_str!("../schemas/reviewer.json");
const REVIEWEE_SCHEMA: &str = include_str!("../schemas/reviewee.json");

/// Claude Code adapter
pub struct ClaudeAdapter {
    reviewer_session_id: Option<String>,
    reviewee_session_id: Option<String>,
    event_sender: Option<mpsc::Sender<RallyEvent>>,
}

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self {
            reviewer_session_id: None,
            reviewee_session_id: None,
            event_sender: None,
        }
    }

    async fn send_event(&self, event: RallyEvent) {
        if let Some(ref sender) = self.event_sender {
            let _ = sender.send(event).await;
        }
    }

    async fn run_claude_streaming(
        &self,
        prompt: &str,
        schema: &str,
        allowed_tools: &str,
        working_dir: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<ClaudeResponse> {
        let mut cmd = Command::new("claude");
        cmd.arg("-p").arg(prompt);
        cmd.arg("--output-format").arg("stream-json");
        cmd.arg("--json-schema").arg(schema);
        cmd.arg("--allowedTools").arg(allowed_tools);

        if let Some(session) = session_id {
            cmd.arg("--resume").arg(session);
        }

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn claude process")?;

        let stdout = child.stdout.take().expect("stdout should be available");
        let stderr = child.stderr.take().expect("stderr should be available");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut final_response: Option<ClaudeResponse> = None;
        let mut error_lines = Vec::new();

        // Process NDJSON stream
        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            if l.trim().is_empty() {
                                continue;
                            }
                            // Try to parse as stream event
                            if let Ok(event) = serde_json::from_str::<StreamEvent>(&l) {
                                self.handle_stream_event(&event).await;

                                // Check if this is the final result
                                if event.event_type == "result" {
                                    // --json-schema uses structured_output, otherwise use result
                                    let result_value = event
                                        .structured_output
                                        .clone()
                                        .or_else(|| event.result.clone());
                                    if let Some(result) = result_value {
                                        final_response = Some(ClaudeResponse {
                                            session_id: event.session_id.unwrap_or_default(),
                                            result: Some(result),
                                            cost_usd: event.cost_usd,
                                            duration_ms: event.duration_ms,
                                        });
                                    }
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => return Err(anyhow!("Error reading stdout: {}", e)),
                    }
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => error_lines.push(l),
                        Ok(None) => {},
                        Err(e) => return Err(anyhow!("Error reading stderr: {}", e)),
                    }
                }
            }
        }

        let status = child
            .wait()
            .await
            .context("Failed to wait for claude process")?;

        if !status.success() {
            let stderr_output = error_lines.join("\n");
            return Err(anyhow!(
                "Claude process failed with status {}: {}",
                status,
                stderr_output
            ));
        }

        final_response.ok_or_else(|| anyhow!("No result received from claude"))
    }

    async fn handle_stream_event(&self, event: &StreamEvent) {
        match event.event_type.as_str() {
            "assistant" => {
                // Assistant message event - may contain thinking or text
                if let Some(ref message) = event.message {
                    for content in &message.content {
                        match content.content_type.as_str() {
                            "thinking" => {
                                if let Some(ref thinking) = content.thinking {
                                    self.send_event(RallyEvent::AgentThinking(thinking.clone()))
                                        .await;
                                }
                            }
                            "text" => {
                                if let Some(ref text) = content.text {
                                    self.send_event(RallyEvent::AgentText(text.clone())).await;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "content_block_start" => {
                if let Some(ref content_block) = event.content_block {
                    match content_block.block_type.as_str() {
                        "tool_use" => {
                            if let Some(ref name) = content_block.name {
                                self.send_event(RallyEvent::AgentToolUse(
                                    name.clone(),
                                    "starting...".to_string(),
                                ))
                                .await;
                            }
                        }
                        "thinking" => {
                            self.send_event(RallyEvent::AgentThinking("Thinking...".to_string()))
                                .await;
                        }
                        _ => {}
                    }
                }
            }
            "content_block_delta" => {
                if let Some(ref delta) = event.delta {
                    match delta.delta_type.as_str() {
                        "thinking_delta" => {
                            if let Some(ref thinking) = delta.thinking {
                                self.send_event(RallyEvent::AgentThinking(thinking.clone()))
                                    .await;
                            }
                        }
                        "text_delta" => {
                            if let Some(ref text) = delta.text {
                                self.send_event(RallyEvent::AgentText(text.clone())).await;
                            }
                        }
                        "input_json_delta" => {
                            // Tool input being streamed - we can optionally show this
                        }
                        _ => {}
                    }
                }
            }
            "content_block_stop" => {
                // Content block completed
            }
            "tool_use" => {
                // Full tool use event
                if let Some(ref name) = event.tool_name {
                    let input_summary = event
                        .tool_input
                        .as_ref()
                        .map(summarize_json)
                        .unwrap_or_default();
                    self.send_event(RallyEvent::AgentToolUse(name.clone(), input_summary))
                        .await;
                }
            }
            "tool_result" => {
                // Tool result event
                if let Some(ref name) = event.tool_name {
                    let result_summary = event
                        .tool_result
                        .as_ref()
                        .map(|s| summarize_text(s))
                        .unwrap_or_else(|| "completed".to_string());
                    self.send_event(RallyEvent::AgentToolResult(name.clone(), result_summary))
                        .await;
                }
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    async fn continue_session(&self, session_id: &str, message: &str) -> Result<ClaudeResponse> {
        let mut cmd = Command::new("claude");
        cmd.arg("-p").arg(message);
        cmd.arg("--resume").arg(session_id);
        cmd.arg("--output-format").arg("json");

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.context("Failed to execute claude")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Claude process failed: {}", stderr));
        }

        let response: ClaudeResponse = serde_json::from_slice(&output.stdout)
            .context("Failed to parse claude output as JSON")?;

        Ok(response)
    }
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentAdapter for ClaudeAdapter {
    fn name(&self) -> &str {
        "claude"
    }

    fn set_event_sender(&mut self, sender: mpsc::Sender<RallyEvent>) {
        self.event_sender = Some(sender);
    }

    async fn run_reviewer(&mut self, prompt: &str, context: &Context) -> Result<ReviewerOutput> {
        // Reviewer tools: read-only operations for code review
        // - Read/Glob/Grep: File reading and searching
        // - gh pr view/diff/checks: View PR information
        // - gh api (GET only): Read-only API calls
        //   Note: We require explicit --method GET or -X GET to prevent POST/PUT/DELETE operations.
        //   The pattern `gh api repos:*` was too permissive as it allowed write operations.
        //
        //   LIMITATION: The gh CLI does not validate flag ordering, so a malicious prompt could
        //   potentially craft commands like `gh api --method GET /endpoint --method POST`.
        //   This is considered acceptable risk as: (1) the reviewer agent has no incentive to
        //   do this, and (2) the model is instructed to only perform read operations.
        let allowed_tools = "Read,Glob,Grep,Bash(gh pr view:*),Bash(gh pr diff:*),Bash(gh pr checks:*),Bash(gh api --method GET:*),Bash(gh api -X GET:*)";

        let response = self
            .run_claude_streaming(
                prompt,
                REVIEWER_SCHEMA,
                allowed_tools,
                context.working_dir.as_deref(),
                None,
            )
            .await?;

        self.reviewer_session_id = Some(response.session_id.clone());

        parse_reviewer_output(&response)
    }

    async fn run_reviewee(&mut self, prompt: &str, context: &Context) -> Result<RevieweeOutput> {
        // Reviewee tools: file editing, safe build/test commands, git push (no --force)
        // Explicitly list safe subcommands to prevent dangerous operations like:
        // - git push --force, git reset --hard
        // - git checkout -- . (discards all changes)
        // - git restore . (discards all changes)
        // - npm publish, pnpm publish, bun publish
        // - cargo publish
        // - cargo clean (could delete build artifacts unexpectedly)
        // - gh pr close/merge/edit (could modify PR state unexpectedly)
        //
        // Note: git push is allowed but --force/-f is prohibited via prompt.
        // Claude Code's permission system doesn't distinguish subflags,
        // so we rely on the prompt to prevent dangerous flags.
        //
        // GitHub CLI: Only safe, read-only PR operations (view, diff, checks) are allowed.
        // Excluded dangerous commands: gh pr close, gh pr merge, gh pr edit, gh pr ready, gh pr reopen
        // API calls require explicit --method GET or -X GET to prevent write operations.
        //
        // KNOWN RISK: Commands like `npm run`, `pnpm run`, `bun run` execute arbitrary scripts
        // defined in package.json. This is an inherent risk but necessary for running tests
        // and build commands. The user should review package.json scripts in the PR.
        let allowed_tools = concat!(
            "Read,Edit,Write,Glob,Grep,",
            // Git: local operations + push (no destructive operations)
            // Note: git checkout and git restore are excluded because they can discard changes
            // (e.g., "git checkout -- ." or "git restore ."). Use git switch for branch operations.
            // git push is allowed but --force/-f is prohibited via prompt instructions.
            "Bash(git status:*),Bash(git diff:*),Bash(git add:*),Bash(git commit:*),",
            "Bash(git log:*),Bash(git show:*),Bash(git branch:*),Bash(git switch:*),",
            "Bash(git stash:*),Bash(git push:*),",
            // GitHub CLI: Only safe, read-only PR operations (view, diff, checks)
            "Bash(gh pr view:*),Bash(gh pr diff:*),Bash(gh pr checks:*),",
            // GitHub API: Only GET requests (read-only)
            "Bash(gh api --method GET:*),Bash(gh api -X GET:*),",
            // Cargo: build, test, check, clippy, fmt (no publish)
            "Bash(cargo build:*),Bash(cargo test:*),Bash(cargo check:*),",
            "Bash(cargo clippy:*),Bash(cargo fmt:*),Bash(cargo run:*),",
            // npm: install, test, build, run (no publish)
            "Bash(npm install:*),Bash(npm test:*),Bash(npm run:*),Bash(npm ci:*),",
            // pnpm: install, test, build, run (no publish)
            "Bash(pnpm install:*),Bash(pnpm test:*),Bash(pnpm run:*),",
            // bun: install, test, build, run (no publish)
            "Bash(bun install:*),Bash(bun test:*),Bash(bun run:*)"
        );

        let response = self
            .run_claude_streaming(
                prompt,
                REVIEWEE_SCHEMA,
                allowed_tools,
                context.working_dir.as_deref(),
                None,
            )
            .await?;

        self.reviewee_session_id = Some(response.session_id.clone());

        parse_reviewee_output(&response)
    }

    async fn continue_reviewer(&mut self, message: &str) -> Result<ReviewerOutput> {
        let session_id = self
            .reviewer_session_id
            .as_ref()
            .ok_or_else(|| anyhow!("No reviewer session to continue"))?
            .clone();

        let response = self.continue_session(&session_id, message).await?;
        parse_reviewer_output(&response)
    }

    async fn continue_reviewee(&mut self, message: &str) -> Result<RevieweeOutput> {
        let session_id = self
            .reviewee_session_id
            .as_ref()
            .ok_or_else(|| anyhow!("No reviewee session to continue"))?
            .clone();

        let response = self.continue_session(&session_id, message).await?;
        parse_reviewee_output(&response)
    }
}

/// Stream event from Claude CLI stream-json output
#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    message: Option<StreamMessage>,
    #[serde(default)]
    content_block: Option<ContentBlock>,
    #[serde(default)]
    delta: Option<Delta>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<serde_json::Value>,
    #[serde(default)]
    tool_result: Option<String>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    /// structured_output is used when --json-schema is specified
    #[serde(default)]
    structured_output: Option<serde_json::Value>,
    #[serde(default)]
    cost_usd: Option<f64>,
    #[serde(default)]
    duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
    #[serde(default)]
    content: Vec<MessageContent>,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
}

/// Claude Code JSON output format
#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    session_id: String,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    #[allow(dead_code)]
    cost_usd: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    duration_ms: Option<u64>,
}

/// Raw reviewer output from Claude
#[derive(Debug, Deserialize)]
struct RawReviewerOutput {
    action: String,
    summary: String,
    comments: Vec<RawReviewComment>,
    blocking_issues: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawReviewComment {
    path: String,
    line: u32,
    body: String,
    severity: String,
}

/// Raw reviewee output from Claude
#[derive(Debug, Deserialize)]
struct RawRevieweeOutput {
    status: String,
    summary: String,
    files_modified: Vec<String>,
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    permission_request: Option<RawPermissionRequest>,
    #[serde(default)]
    error_details: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPermissionRequest {
    action: String,
    reason: String,
}

fn parse_reviewer_output(response: &ClaudeResponse) -> Result<ReviewerOutput> {
    let result = response
        .result
        .as_ref()
        .ok_or_else(|| anyhow!("No result in claude response"))?;

    let raw: RawReviewerOutput =
        serde_json::from_value(result.clone()).context("Failed to parse reviewer output")?;

    let action = match raw.action.as_str() {
        "approve" => ReviewAction::Approve,
        "request_changes" => ReviewAction::RequestChanges,
        "comment" => ReviewAction::Comment,
        _ => return Err(anyhow!("Unknown review action: {}", raw.action)),
    };

    let comments = raw
        .comments
        .into_iter()
        .map(|c| {
            let severity = match c.severity.as_str() {
                "critical" => crate::ai::adapter::CommentSeverity::Critical,
                "major" => crate::ai::adapter::CommentSeverity::Major,
                "minor" => crate::ai::adapter::CommentSeverity::Minor,
                "suggestion" => crate::ai::adapter::CommentSeverity::Suggestion,
                _ => crate::ai::adapter::CommentSeverity::Minor,
            };
            ReviewComment {
                path: c.path,
                line: c.line,
                body: c.body,
                severity,
            }
        })
        .collect();

    Ok(ReviewerOutput {
        action,
        summary: raw.summary,
        comments,
        blocking_issues: raw.blocking_issues,
    })
}

fn parse_reviewee_output(response: &ClaudeResponse) -> Result<RevieweeOutput> {
    let result = response
        .result
        .as_ref()
        .ok_or_else(|| anyhow!("No result in claude response"))?;

    let raw: RawRevieweeOutput =
        serde_json::from_value(result.clone()).context("Failed to parse reviewee output")?;

    let status = match raw.status.as_str() {
        "completed" => RevieweeStatus::Completed,
        "needs_clarification" => RevieweeStatus::NeedsClarification,
        "needs_permission" => RevieweeStatus::NeedsPermission,
        "error" => RevieweeStatus::Error,
        _ => return Err(anyhow!("Unknown reviewee status: {}", raw.status)),
    };

    let permission_request = raw.permission_request.map(|p| PermissionRequest {
        action: p.action,
        reason: p.reason,
    });

    Ok(RevieweeOutput {
        status,
        summary: raw.summary,
        files_modified: raw.files_modified,
        question: raw.question,
        permission_request,
        error_details: raw.error_details,
    })
}

/// Summarize JSON value for display
fn summarize_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let keys: Vec<_> = map.keys().take(3).cloned().collect();
            if keys.is_empty() {
                "{}".to_string()
            } else {
                format!("{{{}: ...}}", keys.join(", "))
            }
        }
        serde_json::Value::String(s) => summarize_text(s),
        serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
        _ => value.to_string(),
    }
}

/// Summarize text for display (UTF-8 safe)
fn summarize_text(s: &str) -> String {
    let s = s.trim();
    let char_count = s.chars().count();
    if char_count <= 60 {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(57).collect();
        format!("{}...", truncated)
    }
}
