//! Common types and parsing functions shared between AI adapters (Claude, Codex, etc.)

use anyhow::{anyhow, Context as AnyhowContext, Result};
use serde::Deserialize;

use crate::ai::adapter::{
    CommentSeverity, PermissionRequest, ReviewAction, ReviewComment, RevieweeOutput,
    RevieweeStatus, ReviewerOutput,
};

/// Raw reviewer output structure shared by all adapters.
#[derive(Debug, Deserialize)]
pub(crate) struct RawReviewerOutput {
    pub action: String,
    pub summary: String,
    pub comments: Vec<RawReviewComment>,
    pub blocking_issues: Vec<String>,
}

/// Raw review comment structure.
#[derive(Debug, Deserialize)]
pub(crate) struct RawReviewComment {
    pub path: String,
    pub line: u32,
    pub body: String,
    pub severity: String,
}

/// Raw reviewee output structure shared by all adapters.
#[derive(Debug, Deserialize)]
pub(crate) struct RawRevieweeOutput {
    pub status: String,
    pub summary: String,
    pub files_modified: Vec<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub permission_request: Option<RawPermissionRequest>,
    #[serde(default)]
    pub error_details: Option<String>,
}

/// Raw permission request structure.
#[derive(Debug, Deserialize)]
pub(crate) struct RawPermissionRequest {
    pub action: String,
    pub reason: String,
}

/// Parse reviewer output from a JSON result value.
///
/// `agent_name` is used in error messages (e.g., "claude", "codex").
pub(crate) fn parse_reviewer_output(
    result: Option<&serde_json::Value>,
    agent_name: &str,
) -> Result<ReviewerOutput> {
    let result = result.ok_or_else(|| anyhow!("No result in {} response", agent_name))?;

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

/// Parse reviewee output from a JSON result value.
///
/// `agent_name` is used in error messages (e.g., "claude", "codex").
pub(crate) fn parse_reviewee_output(
    result: Option<&serde_json::Value>,
    agent_name: &str,
) -> Result<RevieweeOutput> {
    let result = result.ok_or_else(|| anyhow!("No result in {} response", agent_name))?;

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
pub(super) fn summarize_json(value: &serde_json::Value) -> String {
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
pub(super) fn summarize_text(s: &str) -> String {
    let s = s.trim();
    let char_count = s.chars().count();
    if char_count <= 60 {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(57).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_json_snapshot;

    // --- parse_reviewer_output tests ---

    #[test]
    fn test_parse_reviewer_output_request_changes() {
        let result = serde_json::json!({
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
        });

        let output = parse_reviewer_output(Some(&result), "test").unwrap();
        assert_json_snapshot!(output, @r#"
        {
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
          "blocking_issues": [
            "Missing error handling"
          ]
        }
        "#);
    }

    #[test]
    fn test_parse_reviewer_output_approve() {
        let result = serde_json::json!({
            "action": "approve",
            "summary": "LGTM",
            "comments": [],
            "blocking_issues": []
        });

        let output = parse_reviewer_output(Some(&result), "test").unwrap();
        assert_json_snapshot!(output, @r#"
        {
          "action": "approve",
          "summary": "LGTM",
          "comments": [],
          "blocking_issues": []
        }
        "#);
    }

    #[test]
    fn test_parse_reviewee_output_completed() {
        let result = serde_json::json!({
            "status": "completed",
            "summary": "Fixed all issues",
            "files_modified": ["src/lib.rs", "src/main.rs"]
        });

        // Note: question, permission_request, error_details are None
        // and have #[serde(skip_serializing_if = "Option::is_none")], so they won't appear in JSON
        let output = parse_reviewee_output(Some(&result), "test").unwrap();
        assert_json_snapshot!(output, @r#"
        {
          "status": "completed",
          "summary": "Fixed all issues",
          "files_modified": [
            "src/lib.rs",
            "src/main.rs"
          ]
        }
        "#);
    }

    #[test]
    fn test_parse_reviewee_output_needs_permission() {
        let result = serde_json::json!({
            "status": "needs_permission",
            "summary": "Need to run a command",
            "files_modified": [],
            "permission_request": {
                "action": "run npm install",
                "reason": "Required to install new dependency"
            }
        });

        // Note: question and error_details are None and skipped via skip_serializing_if
        let output = parse_reviewee_output(Some(&result), "test").unwrap();
        assert_json_snapshot!(output, @r#"
        {
          "status": "needs_permission",
          "summary": "Need to run a command",
          "files_modified": [],
          "permission_request": {
            "action": "run npm install",
            "reason": "Required to install new dependency"
          }
        }
        "#);
    }

    // --- Error path tests ---

    #[test]
    fn test_parse_reviewer_output_none_result() {
        let err = parse_reviewer_output(None, "test").unwrap_err();
        assert!(err.to_string().contains("No result in test response"));
    }

    #[test]
    fn test_parse_reviewer_output_unknown_action() {
        let result = serde_json::json!({
            "action": "reject",
            "summary": "Bad",
            "comments": [],
            "blocking_issues": []
        });

        let err = parse_reviewer_output(Some(&result), "test").unwrap_err();
        assert!(err.to_string().contains("Unknown review action: reject"));
    }

    #[test]
    fn test_parse_reviewee_output_unknown_status() {
        let result = serde_json::json!({
            "status": "pending",
            "summary": "Waiting",
            "files_modified": []
        });

        let err = parse_reviewee_output(Some(&result), "test").unwrap_err();
        assert!(err.to_string().contains("Unknown reviewee status: pending"));
    }

    #[test]
    fn test_parse_reviewer_output_unknown_severity_fallback() {
        let result = serde_json::json!({
            "action": "comment",
            "summary": "Review",
            "comments": [
                {
                    "path": "src/lib.rs",
                    "line": 10,
                    "body": "Check this",
                    "severity": "unknown_severity"
                }
            ],
            "blocking_issues": []
        });

        let output = parse_reviewer_output(Some(&result), "test").unwrap();
        // Unknown severity falls back to Minor
        assert_eq!(output.comments[0].severity, CommentSeverity::Minor);
    }

    // --- Utility tests ---

    #[test]
    fn test_summarize_json_object() {
        let value = serde_json::json!({"key1": "val1", "key2": "val2"});
        let summary = summarize_json(&value);
        assert!(summary.contains("key1"));
        assert!(summary.contains("key2"));
        assert!(summary.contains("..."));
    }

    #[test]
    fn test_summarize_json_array() {
        let value = serde_json::json!([1, 2, 3]);
        assert_eq!(summarize_json(&value), "[3 items]");
    }

    #[test]
    fn test_summarize_text_short() {
        assert_eq!(summarize_text("hello"), "hello");
    }

    #[test]
    fn test_summarize_text_long() {
        let long = "a".repeat(100);
        let summary = summarize_text(&long);
        assert!(summary.ends_with("..."));
        assert_eq!(summary.chars().count(), 60); // 57 + "..."
    }
}
