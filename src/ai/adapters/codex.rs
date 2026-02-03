use anyhow::{anyhow, Context as AnyhowContext, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::io::Write;
use std::process::Stdio;
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::ai::adapter::{
    AgentAdapter, CommentSeverity, Context, PermissionRequest, ReviewAction, ReviewComment,
    RevieweeOutput, RevieweeStatus, ReviewerOutput,
};
use crate::ai::orchestrator::RallyEvent;

// Codex requires additionalProperties: false for all objects in the schema
const REVIEWER_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ReviewerOutput",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "action": {
      "type": "string",
      "enum": ["approve", "request_changes", "comment"]
    },
    "summary": {
      "type": "string"
    },
    "comments": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "path": {"type": "string"},
          "line": {"type": "integer"},
          "body": {"type": "string"},
          "severity": {"type": "string", "enum": ["critical", "major", "minor", "suggestion"]}
        },
        "required": ["path", "line", "body", "severity"]
      }
    },
    "blocking_issues": {
      "type": "array",
      "items": {"type": "string"}
    }
  },
  "required": ["action", "summary", "comments", "blocking_issues"]
}"#;

const REVIEWEE_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "RevieweeOutput",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "status": {
      "type": "string",
      "enum": ["completed", "needs_clarification", "needs_permission", "error"]
    },
    "summary": {
      "type": "string"
    },
    "files_modified": {
      "type": "array",
      "items": {"type": "string"}
    },
    "question": {
      "type": "string"
    },
    "permission_request": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "action": {"type": "string"},
        "reason": {"type": "string"}
      },
      "required": ["action", "reason"]
    },
    "error_details": {
      "type": "string"
    }
  },
  "required": ["status", "summary", "files_modified"]
}"#;

/// Codex-specific errors
#[derive(Debug, Error)]
pub enum CodexError {
    #[error("Codex CLI not found. Install it with: npm install -g @openai/codex")]
    #[allow(dead_code)]
    CliNotFound,
    #[error("Codex authentication failed. Run 'codex auth' to authenticate")]
    AuthenticationFailed,
    #[error("Turn failed: {reason}")]
    TurnFailed { reason: String },
    #[error("Invalid JSON event: {0}")]
    #[allow(dead_code)]
    InvalidJsonEvent(#[from] serde_json::Error),
    #[error("Event channel closed")]
    #[allow(dead_code)]
    ChannelClosed,
}

/// OpenAI Codex CLI adapter
pub struct CodexAdapter {
    reviewer_session_id: Option<String>,
    reviewee_session_id: Option<String>,
    event_sender: Option<mpsc::Sender<RallyEvent>>,
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self {
            reviewer_session_id: None,
            reviewee_session_id: None,
            event_sender: None,
        }
    }

    /// Check if Codex CLI is available
    #[allow(dead_code)]
    pub fn check_availability() -> Result<(), CodexError> {
        let output = std::process::Command::new("codex")
            .arg("--version")
            .output();

        match output {
            Ok(o) if o.status.success() => Ok(()),
            _ => Err(CodexError::CliNotFound),
        }
    }

    async fn send_event(&self, event: RallyEvent) {
        if let Some(ref sender) = self.event_sender {
            let _ = sender.send(event).await;
        }
    }

    /// Run Codex CLI with streaming JSON output
    ///
    /// If `session_id` is provided (resume case), and Codex does not emit a new
    /// `thread.started` event, the existing session_id is preserved in the response.
    async fn run_codex_streaming(
        &self,
        prompt: &str,
        schema: &str,
        full_auto: bool,
        working_dir: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<CodexResponse> {
        // Write schema to temporary file (Codex requires file path for --output-schema)
        let mut schema_file =
            NamedTempFile::new().context("Failed to create temporary schema file")?;
        schema_file
            .write_all(schema.as_bytes())
            .context("Failed to write schema to temporary file")?;

        let mut cmd = Command::new("codex");

        // Handle session resume
        // Usage: codex exec resume <SESSION_ID> [PROMPT]
        if let Some(sid) = session_id {
            cmd.arg("exec").arg("resume").arg(sid).arg(prompt);
        } else {
            cmd.arg("exec").arg(prompt);
        }

        cmd.arg("--json");
        cmd.arg("--output-schema").arg(schema_file.path());

        // Set working directory
        if let Some(dir) = working_dir {
            cmd.arg("--cd").arg(dir);
        }

        // Set sandbox mode
        // - Reviewer: default (read-only)
        // - Reviewee: --full-auto (workspace-write)
        if full_auto {
            cmd.arg("--full-auto");
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().context("Failed to spawn codex process")?;

        let stdout = child.stdout.take().expect("stdout should be available");
        let stderr = child.stderr.take().expect("stderr should be available");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut final_response: Option<CodexResponse> = None;
        let mut error_lines = Vec::new();
        // Initialize thread_id with existing session_id for resume case
        // This ensures we don't lose the session if Codex doesn't emit thread.started
        let mut thread_id: Option<String> = session_id.map(|s| s.to_string());
        let mut stream_error: Option<anyhow::Error> = None;

        // Process NDJSON stream
        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            if l.trim().is_empty() {
                                continue;
                            }
                            // Parse Codex event
                            match serde_json::from_str::<CodexEvent>(&l) {
                                Ok(event) => {
                                    match self.handle_codex_event(&event, &mut thread_id).await {
                                        Ok(Some(result)) => {
                                            final_response = Some(result);
                                        }
                                        Ok(None) => {}
                                        Err(e) => {
                                            // Capture error but continue to wait for process
                                            stream_error = Some(e);
                                            break;
                                        }
                                    }
                                }
                                Err(_) => {
                                    // Unknown event format, ignore
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            stream_error = Some(anyhow!("Error reading stdout: {}", e));
                            break;
                        }
                    }
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => error_lines.push(l),
                        Ok(None) => {},
                        Err(e) => {
                            stream_error = Some(anyhow!("Error reading stderr: {}", e));
                            break;
                        }
                    }
                }
            }
        }

        // Always wait for the child process to terminate before returning
        // This ensures we don't leave zombie processes and the temp schema file
        // is only deleted after the process has finished
        let status = match child.wait().await {
            Ok(s) => s,
            Err(e) => {
                // If wait fails, try to kill the process
                let _ = child.kill().await;
                return Err(anyhow!("Failed to wait for codex process: {}", e));
            }
        };

        // Now that child has terminated, return any captured stream error
        if let Some(e) = stream_error {
            return Err(e);
        }

        // schema_file is dropped here and the temporary file is deleted

        if !status.success() {
            let stderr_output = error_lines.join("\n");

            // Check for authentication error
            if stderr_output.contains("auth") || stderr_output.contains("unauthorized") {
                return Err(CodexError::AuthenticationFailed.into());
            }

            return Err(anyhow!(
                "Codex process failed with status {}: {}",
                status,
                stderr_output
            ));
        }

        final_response.ok_or_else(|| anyhow!("No result received from codex"))
    }

    /// Handle Codex streaming event and convert to RallyEvent
    async fn handle_codex_event(
        &self,
        event: &CodexEvent,
        thread_id: &mut Option<String>,
    ) -> Result<Option<CodexResponse>> {
        match event {
            CodexEvent::ThreadStarted { thread_id: tid } => {
                *thread_id = Some(tid.clone());
                self.send_event(RallyEvent::AgentThinking("Starting...".to_string()))
                    .await;
            }
            CodexEvent::TurnStarted => {
                self.send_event(RallyEvent::AgentThinking("Processing...".to_string()))
                    .await;
            }
            CodexEvent::TurnCompleted { .. } => {
                // In Codex CLI, the result comes from item.completed with type=agent_message
                // turn.completed only has usage info, no result
            }
            CodexEvent::TurnFailed { error } => {
                let reason = error
                    .as_ref()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| "Unknown error".to_string());
                return Err(CodexError::TurnFailed { reason }.into());
            }
            CodexEvent::Error { message } => {
                return Err(CodexError::TurnFailed {
                    reason: message.clone(),
                }
                .into());
            }
            CodexEvent::ItemStarted { item } | CodexEvent::ItemUpdated { item } => {
                // Non-completed items don't produce results, but we still propagate errors
                self.handle_item_event(item, thread_id, false).await?;
            }
            CodexEvent::ItemCompleted { item } => {
                // Check if this is the final agent_message with structured output
                if let Some(result) = self.handle_item_event(item, thread_id, true).await? {
                    return Ok(Some(result));
                }
            }
            CodexEvent::Unknown => {
                // Ignore unknown events
            }
        }
        Ok(None)
    }

    /// Handle item events and return result if it's the final agent_message
    ///
    /// Returns `Err` if the final result cannot be constructed (e.g., missing session_id).
    async fn handle_item_event(
        &self,
        item: &CodexItem,
        thread_id: &Option<String>,
        completed: bool,
    ) -> Result<Option<CodexResponse>> {
        match item.item_type.as_str() {
            "reasoning" => {
                // Stream reasoning/thinking content to logs
                if let Some(ref text) = item.text {
                    self.send_event(RallyEvent::AgentThinking(text.clone()))
                        .await;
                }
            }
            "agent_message" => {
                if completed {
                    // The text field contains the JSON result as a string
                    if let Some(ref text) = item.text {
                        // Try to parse as JSON
                        if let Ok(result) = serde_json::from_str::<serde_json::Value>(text) {
                            self.send_event(RallyEvent::AgentText("Review completed.".to_string()))
                                .await;
                            // Ensure we have a valid session_id; error if missing
                            let session_id = thread_id.clone().ok_or_else(|| {
                                anyhow!(
                                    "No session_id available: Codex did not emit thread.started \
                                     and no existing session was provided"
                                )
                            })?;
                            return Ok(Some(CodexResponse {
                                session_id,
                                result: Some(result),
                            }));
                        }
                        // If not JSON, just show as text
                        self.send_event(RallyEvent::AgentText(text.clone())).await;
                    }
                } else if let Some(ref text) = item.text {
                    self.send_event(RallyEvent::AgentThinking(text.clone()))
                        .await;
                }
            }
            "function_call" | "command" => {
                let tool_name = item
                    .name
                    .clone()
                    .or_else(|| item.command.clone())
                    .unwrap_or_else(|| "tool".to_string());

                if completed {
                    let output = item
                        .output
                        .clone()
                        .unwrap_or_else(|| "completed".to_string());
                    self.send_event(RallyEvent::AgentToolResult(tool_name, output))
                        .await;
                } else {
                    self.send_event(RallyEvent::AgentToolUse(
                        tool_name,
                        "running...".to_string(),
                    ))
                    .await;
                }
            }
            "file_edit" | "file_change" => {
                let path = item.path.clone().unwrap_or_else(|| "file".to_string());
                if completed {
                    self.send_event(RallyEvent::AgentToolResult(
                        format!("edit:{}", path),
                        "file modified".to_string(),
                    ))
                    .await;
                } else {
                    self.send_event(RallyEvent::AgentToolUse(
                        format!("edit:{}", path),
                        "modifying...".to_string(),
                    ))
                    .await;
                }
            }
            _ => {}
        }
        Ok(None)
    }
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentAdapter for CodexAdapter {
    fn name(&self) -> &str {
        "codex"
    }

    fn set_event_sender(&mut self, sender: mpsc::Sender<RallyEvent>) {
        self.event_sender = Some(sender);
    }

    async fn run_reviewer(&mut self, prompt: &str, context: &Context) -> Result<ReviewerOutput> {
        // Reviewer runs in default sandbox mode (read-only)
        // Codex doesn't have fine-grained tool control like Claude's --allowedTools
        // Instead, it uses sandbox policies:
        // - default: read-only filesystem access
        // - full-auto: workspace write access
        let response = self
            .run_codex_streaming(
                prompt,
                REVIEWER_SCHEMA,
                false, // read-only sandbox for reviewer
                context.working_dir.as_deref(),
                None,
            )
            .await?;

        self.reviewer_session_id = Some(response.session_id.clone());

        parse_reviewer_output(&response)
    }

    async fn run_reviewee(&mut self, prompt: &str, context: &Context) -> Result<RevieweeOutput> {
        // Reviewee runs in full-auto mode (workspace-write)
        // NOTE: full-auto allows git push, but the prompt explicitly prohibits it
        let response = self
            .run_codex_streaming(
                prompt,
                REVIEWEE_SCHEMA,
                true, // full-auto sandbox for reviewee
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

        let response = self
            .run_codex_streaming(message, REVIEWER_SCHEMA, false, None, Some(&session_id))
            .await?;

        parse_reviewer_output(&response)
    }

    async fn continue_reviewee(&mut self, message: &str) -> Result<RevieweeOutput> {
        let session_id = self
            .reviewee_session_id
            .as_ref()
            .ok_or_else(|| anyhow!("No reviewee session to continue"))?
            .clone();

        let response = self
            .run_codex_streaming(message, REVIEWEE_SCHEMA, true, None, Some(&session_id))
            .await?;

        parse_reviewee_output(&response)
    }

    fn add_reviewee_allowed_tool(&mut self, _tool: &str) {
        // Codex doesn't support granular tool permissions like Claude's --allowedTools.
        // It uses sandbox policies (read-only vs full-auto) instead.
        // This is a no-op for Codex.
    }
}

// Codex event types based on actual CLI output
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum CodexEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted { thread_id: String },
    #[serde(rename = "turn.started")]
    TurnStarted,
    #[serde(rename = "turn.completed")]
    TurnCompleted {
        #[serde(default)]
        #[allow(dead_code)]
        usage: Option<serde_json::Value>,
    },
    #[serde(rename = "turn.failed")]
    TurnFailed {
        #[serde(default)]
        error: Option<CodexErrorInfo>,
    },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "item.started")]
    ItemStarted { item: CodexItem },
    #[serde(rename = "item.updated")]
    ItemUpdated { item: CodexItem },
    #[serde(rename = "item.completed")]
    ItemCompleted { item: CodexItem },
    #[serde(other)]
    Unknown,
}

/// Error info in turn.failed event
#[derive(Debug, Deserialize)]
pub struct CodexErrorInfo {
    #[serde(default)]
    pub message: String,
}

/// Codex item structure (not tagged enum, uses "type" field)
#[derive(Debug, Deserialize)]
pub struct CodexItem {
    #[serde(default)]
    #[allow(dead_code)]
    pub id: Option<String>,
    #[serde(rename = "type", default)]
    pub item_type: String,
    /// Text content for agent_message
    #[serde(default)]
    pub text: Option<String>,
    /// Command string for function_call/command
    #[serde(default)]
    pub command: Option<String>,
    /// Function name for function_call
    #[serde(default)]
    pub name: Option<String>,
    /// Output for completed commands
    #[serde(default)]
    pub output: Option<String>,
    /// File path for file_edit
    #[serde(default)]
    pub path: Option<String>,
}

/// Codex response structure
#[derive(Debug)]
struct CodexResponse {
    session_id: String,
    result: Option<serde_json::Value>,
}

use super::common::{RawRevieweeOutput, RawReviewerOutput};

fn parse_reviewer_output(response: &CodexResponse) -> Result<ReviewerOutput> {
    let result = response
        .result
        .as_ref()
        .ok_or_else(|| anyhow!("No result in codex response"))?;

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
                "critical" => CommentSeverity::Critical,
                "major" => CommentSeverity::Major,
                "minor" => CommentSeverity::Minor,
                "suggestion" => CommentSeverity::Suggestion,
                _ => CommentSeverity::Minor,
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

fn parse_reviewee_output(response: &CodexResponse) -> Result<RevieweeOutput> {
    let result = response
        .result
        .as_ref()
        .ok_or_else(|| anyhow!("No result in codex response"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_thread_started_event() {
        let json = r#"{"type": "thread.started", "thread_id": "thread_123"}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::ThreadStarted { thread_id } => {
                assert_eq!(thread_id, "thread_123");
            }
            _ => panic!("Expected ThreadStarted event"),
        }
    }

    #[test]
    fn test_parse_turn_started_event() {
        let json = r#"{"type": "turn.started"}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexEvent::TurnStarted));
    }

    #[test]
    fn test_parse_turn_completed_event() {
        let json = r#"{"type": "turn.completed", "usage": {"input_tokens": 100}}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexEvent::TurnCompleted { .. }));
    }

    #[test]
    fn test_parse_turn_failed_event() {
        let json = r#"{"type": "turn.failed", "error": {"message": "Something went wrong"}}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::TurnFailed { error } => {
                assert_eq!(error.unwrap().message, "Something went wrong");
            }
            _ => panic!("Expected TurnFailed event"),
        }
    }

    #[test]
    fn test_parse_error_event() {
        let json = r#"{"type": "error", "message": "API error occurred"}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::Error { message } => {
                assert_eq!(message, "API error occurred");
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_parse_item_completed_agent_message() {
        let json = r#"{"type": "item.completed", "item": {"id": "item_0", "type": "agent_message", "text": "{\"action\":\"approve\",\"summary\":\"LGTM\",\"comments\":[],\"blocking_issues\":[]}"}}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::ItemCompleted { item } => {
                assert_eq!(item.item_type, "agent_message");
                assert!(item.text.is_some());
                // Verify the text contains valid JSON
                let text = item.text.unwrap();
                let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(parsed["action"], "approve");
            }
            _ => panic!("Expected ItemCompleted event"),
        }
    }

    #[test]
    fn test_parse_item_function_call() {
        let json = r#"{"type": "item.started", "item": {"id": "item_1", "type": "function_call", "name": "read_file", "command": "cat src/main.rs"}}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        match event {
            CodexEvent::ItemStarted { item } => {
                assert_eq!(item.item_type, "function_call");
                assert_eq!(item.name, Some("read_file".to_string()));
            }
            _ => panic!("Expected ItemStarted event"),
        }
    }

    #[test]
    fn test_parse_unknown_event() {
        let json = r#"{"type": "some.unknown.event", "data": "whatever"}"#;
        let event: CodexEvent = serde_json::from_str(json).unwrap();
        assert!(matches!(event, CodexEvent::Unknown));
    }

    #[test]
    fn test_parse_reviewer_output() {
        let response = CodexResponse {
            session_id: "session_123".to_string(),
            result: Some(serde_json::json!({
                "action": "request_changes",
                "summary": "Found some issues",
                "comments": [
                    {
                        "path": "src/lib.rs",
                        "line": 42,
                        "body": "Consider using a constant here",
                        "severity": "suggestion"
                    }
                ],
                "blocking_issues": ["Missing error handling"]
            })),
        };

        let output = parse_reviewer_output(&response).unwrap();
        assert_eq!(output.action, ReviewAction::RequestChanges);
        assert_eq!(output.summary, "Found some issues");
        assert_eq!(output.comments.len(), 1);
        assert_eq!(output.comments[0].path, "src/lib.rs");
        assert_eq!(output.comments[0].line, 42);
        assert_eq!(output.comments[0].severity, CommentSeverity::Suggestion);
        assert_eq!(output.blocking_issues.len(), 1);
    }

    #[test]
    fn test_parse_reviewee_output() {
        let response = CodexResponse {
            session_id: "session_456".to_string(),
            result: Some(serde_json::json!({
                "status": "completed",
                "summary": "Fixed all issues",
                "files_modified": ["src/lib.rs", "src/main.rs"]
            })),
        };

        let output = parse_reviewee_output(&response).unwrap();
        assert_eq!(output.status, RevieweeStatus::Completed);
        assert_eq!(output.summary, "Fixed all issues");
        assert_eq!(output.files_modified.len(), 2);
    }

    #[test]
    fn test_parse_reviewee_needs_permission() {
        let response = CodexResponse {
            session_id: "session_789".to_string(),
            result: Some(serde_json::json!({
                "status": "needs_permission",
                "summary": "Need to run a command",
                "files_modified": [],
                "permission_request": {
                    "action": "run npm install",
                    "reason": "Required to install new dependency"
                }
            })),
        };

        let output = parse_reviewee_output(&response).unwrap();
        assert_eq!(output.status, RevieweeStatus::NeedsPermission);
        assert!(output.permission_request.is_some());
        let perm = output.permission_request.unwrap();
        assert_eq!(perm.action, "run npm install");
    }
}
