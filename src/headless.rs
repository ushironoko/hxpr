use anyhow::Result;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::ai::adapter::{
    CommentSeverity, Context, ReviewAction, RevieweeOutput, RevieweeStatus, ReviewerOutput,
};
use crate::ai::orchestrator::{Orchestrator, OrchestratorCommand, RallyEvent, RallyState};
use crate::config::Config;
use crate::github;

/// Run AI Rally in headless mode (no TUI).
///
/// Progress logs are written to stderr. On completion, a JSON summary is written to stdout.
/// This is invoked when `--ai-rally --pr <number>` is specified.
/// Returns `true` if approved, `false` otherwise.
pub async fn run_headless_rally(
    repo: &str,
    pr_number: u32,
    config: &Config,
    working_dir: Option<&str>,
) -> Result<bool> {
    eprintln!("[Headless] Fetching PR #{} from {}...", pr_number, repo);

    let pr = github::fetch_pr(repo, pr_number).await?;
    let files = github::fetch_changed_files(repo, pr_number).await?;

    let mut file_patches: Vec<(String, String)> = files
        .iter()
        .filter_map(|f| f.patch.as_ref().map(|p| (f.filename.clone(), p.clone())))
        .collect();

    // Fallback: if some files are missing patches (large files), fetch full PR diff
    let has_missing_patches = files.iter().any(|f| f.patch.is_none());
    if has_missing_patches {
        eprintln!("[Headless] Some files missing patches, fetching full PR diff...");
        if let Ok(full_diff) = github::fetch_pr_diff(repo, pr_number).await {
            let parsed = crate::diff::parse_unified_diff(&full_diff);
            for (filename, patch) in &parsed {
                if !file_patches.iter().any(|(f, _)| f == filename) {
                    file_patches.push((filename.clone(), patch.clone()));
                }
            }
        }
    }

    let diff = file_patches
        .iter()
        .map(|(_, p)| p.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let context = Context {
        repo: repo.to_string(),
        pr_number,
        pr_title: pr.title.clone(),
        pr_body: pr.body.clone(),
        diff,
        working_dir: working_dir.map(|s| s.to_string()),
        head_sha: pr.head.sha.clone(),
        base_branch: pr.base.ref_name.clone(),
        external_comments: Vec::new(),
        local_mode: false,
        file_patches,
    };

    run_headless_with_context(repo, pr_number, config, context).await
}

/// Run AI Rally in headless mode for local diff.
///
/// Progress logs are written to stderr. On completion, a JSON summary is written to stdout.
/// This is invoked when `--local --ai-rally` is specified.
/// Returns `true` if approved, `false` otherwise.
pub async fn run_headless_rally_local(
    repo: &str,
    config: &Config,
    working_dir: Option<&str>,
) -> Result<bool> {
    eprintln!("[Headless] Running local diff rally...");

    let wd = working_dir
        .map(|s| s.to_string())
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string()));

    let dir = wd.as_deref().unwrap_or(".");
    let base_branch = detect_local_base_branch(Some(dir)).unwrap_or_else(|| "main".to_string());

    // Use `git diff HEAD` to capture working tree changes (staged + unstaged)
    let diff_output = tokio::process::Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(dir)
        .output()
        .await?;

    if !diff_output.status.success() {
        let stderr = String::from_utf8_lossy(&diff_output.stderr);
        anyhow::bail!(
            "git diff HEAD failed (exit {}): {}",
            diff_output.status,
            stderr.trim()
        );
    }

    let mut diff = String::from_utf8_lossy(&diff_output.stdout).to_string();

    // Include untracked files (mirrors TUI behavior in loader.rs)
    // Uses `git ls-files --others --exclude-standard` + `git diff --no-index`
    let untracked_diff = collect_untracked_diff(dir).await;
    if !untracked_diff.is_empty() {
        if !diff.is_empty() && !diff.ends_with('\n') {
            diff.push('\n');
        }
        diff.push_str(&untracked_diff);
    }

    // Fallback: if no uncommitted changes, try committed changes vs base branch
    if diff.trim().is_empty() {
        eprintln!(
            "[Headless] No uncommitted changes, trying diff against origin/{}...",
            base_branch
        );
        let fallback_output = tokio::process::Command::new("git")
            .args([
                "diff",
                &format!("origin/{}...HEAD", base_branch),
            ])
            .current_dir(dir)
            .output()
            .await?;

        if fallback_output.status.success() {
            diff = String::from_utf8_lossy(&fallback_output.stdout).to_string();
        } else {
            let stderr = String::from_utf8_lossy(&fallback_output.stderr);
            eprintln!(
                "[Headless] Fallback diff also failed: {}",
                stderr.trim()
            );
        }
    }

    if diff.trim().is_empty() {
        anyhow::bail!(
            "No changes detected: both git diff HEAD and git diff origin/{}...HEAD returned empty",
            base_branch
        );
    }

    let head_sha = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let context = Context {
        repo: repo.to_string(),
        pr_number: 0,
        pr_title: "Local diff".to_string(),
        pr_body: None,
        diff,
        working_dir: wd,
        head_sha,
        base_branch,
        external_comments: Vec::new(),
        local_mode: true,
        file_patches: Vec::new(),
    };

    run_headless_with_context(repo, 0, config, context).await
}

/// Core headless execution logic shared between PR and local modes.
async fn run_headless_with_context(
    repo: &str,
    pr_number: u32,
    config: &Config,
    context: Context,
) -> Result<bool> {
    let (event_tx, mut event_rx) = mpsc::channel(100);
    let (cmd_tx, cmd_rx) = mpsc::channel(10);

    let mut orchestrator =
        Orchestrator::new(repo, pr_number, config.ai.clone(), event_tx, Some(cmd_rx))?;
    orchestrator.set_context(context);

    // Spawn orchestrator in background
    let orchestrator_handle = tokio::spawn(async move { orchestrator.run().await });

    // Event loop: receive events and auto-respond to interactive requests
    let outcome = run_headless_event_loop(&mut event_rx, &cmd_tx).await;

    // Wait for orchestrator to finish
    let _ = orchestrator_handle.await;

    // Existing stderr output (unchanged)
    match &outcome.result {
        HeadlessResult::Approved(_) => {
            eprintln!("\n[Headless] Rally completed: Approved");
        }
        HeadlessResult::NotApproved(reason) => {
            eprintln!("\n[Headless] Rally completed: {}", reason);
        }
        HeadlessResult::Error(msg) => {
            eprintln!("\n[Headless] Rally error: {}", msg);
        }
    }

    // JSON output to stdout
    let json_output = build_json_output(&outcome);
    write_json_stdout(&json_output);

    Ok(matches!(outcome.result, HeadlessResult::Approved(_)))
}

enum HeadlessResult {
    Approved(String),
    NotApproved(String),
    Error(String),
}

/// Internal outcome from the headless event loop, bundling result with collected data.
struct HeadlessOutcome {
    result: HeadlessResult,
    iterations: u32,
    last_review: Option<ReviewerOutput>,
    last_fix: Option<RevieweeOutput>,
}

/// Result kind for JSON output (serialized as snake_case).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum HeadlessResultKind {
    Approved,
    NotApproved,
    Error,
}

/// JSON output structure written to stdout on headless completion.
#[derive(Debug, Serialize)]
struct HeadlessJsonOutput {
    result: HeadlessResultKind,
    iterations: u32,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_review: Option<ReviewerOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_fix: Option<RevieweeOutput>,
}

/// Event loop that processes RallyEvents and auto-responds to interactive requests.
///
/// Headless policy:
/// - Clarification: auto-skip (continue with best judgment)
/// - Permission: auto-deny (prevents dynamic tool expansion without human review)
/// - PostConfirmation: respect auto_post config; if auto_post=false, auto-approve
/// - AgentText: suppressed (prevents structured output JSON leakage)
/// - AgentThinking: suppressed (noise reduction)
async fn run_headless_event_loop(
    event_rx: &mut mpsc::Receiver<RallyEvent>,
    cmd_tx: &mpsc::Sender<OrchestratorCommand>,
) -> HeadlessOutcome {
    let mut last_error: Option<String> = None;
    let mut current_iteration: u32 = 0;
    let mut last_review: Option<ReviewerOutput> = None;
    let mut last_fix: Option<RevieweeOutput> = None;

    while let Some(event) = event_rx.recv().await {
        match event {
            RallyEvent::IterationStarted(n) => {
                current_iteration = n;
                eprintln!("\n=== Iteration {} ===", n);
            }
            RallyEvent::StateChanged(state) => match state {
                RallyState::ReviewerReviewing => {
                    eprintln!("[Reviewer] Reviewing...");
                }
                RallyState::RevieweeFix => {
                    eprintln!("[Reviewee] Fixing...");
                }
                RallyState::Completed => {
                    // Will be handled by Approved event
                }
                RallyState::Aborted => {
                    return HeadlessOutcome {
                        result: HeadlessResult::NotApproved(
                            last_error.unwrap_or_else(|| "Aborted".to_string()),
                        ),
                        iterations: current_iteration,
                        last_review,
                        last_fix,
                    };
                }
                RallyState::Error => {
                    return HeadlessOutcome {
                        result: HeadlessResult::Error(
                            last_error.unwrap_or_else(|| "Unknown error".to_string()),
                        ),
                        iterations: current_iteration,
                        last_review,
                        last_fix,
                    };
                }
                _ => {}
            },
            RallyEvent::ReviewCompleted(output) => {
                eprintln!("{}", format_review_output(&output));
                last_review = Some(output);
            }
            RallyEvent::FixCompleted(output) => {
                eprintln!("{}", format_fix_output(&output));
                last_fix = Some(output);
            }
            RallyEvent::Approved(summary) => {
                eprintln!("\n[Approved] {}", summary);
                return HeadlessOutcome {
                    result: HeadlessResult::Approved(summary),
                    iterations: current_iteration,
                    last_review,
                    last_fix,
                };
            }
            RallyEvent::Error(msg) => {
                eprintln!("\n[Error] {}", msg);
                last_error = Some(msg);
            }
            RallyEvent::Log(msg) => {
                eprintln!("  {}", msg);
            }
            RallyEvent::AgentToolUse(name, _input) => {
                eprintln!("  > {}", name);
            }
            RallyEvent::AgentToolResult(name, result) => {
                let truncated = truncate_str(&result, 200);
                eprintln!("  < {}: {}", name, truncated);
            }
            // Suppress AgentThinking and AgentText to prevent noise and JSON leakage
            RallyEvent::AgentThinking(_) | RallyEvent::AgentText(_) => {}
            // Auto-skip clarification (headless can't interact)
            RallyEvent::ClarificationNeeded(question) => {
                eprintln!("  [Clarification needed] {}", question);
                eprintln!("  -> Auto-skipping (headless mode)");
                let _ = cmd_tx.send(OrchestratorCommand::SkipClarification).await;
            }
            // Deny permission by default in headless mode.
            // Auto-approving would allow dynamic tool expansion via prompt injection,
            // bypassing the allowedTools constraint. Deny-by-default is the safe choice
            // for non-interactive CI runs.
            RallyEvent::PermissionNeeded(action, reason) => {
                eprintln!("  [Permission needed] {}: {}", action, reason);
                eprintln!("  -> Auto-denying (headless mode, no human to confirm)");
                let _ = cmd_tx
                    .send(OrchestratorCommand::PermissionResponse(false))
                    .await;
            }
            // Post confirmation: always approve in headless mode
            // (auto_post is checked by orchestrator; if we receive this event,
            // it means auto_post is false, and we approve since headless can't interact)
            RallyEvent::ReviewPostConfirmNeeded(info) => {
                eprintln!(
                    "  [Post review] {}: {} ({} comments)",
                    info.action, info.summary, info.comment_count
                );
                eprintln!("  -> Auto-approving post (headless mode)");
                let _ = cmd_tx
                    .send(OrchestratorCommand::PostConfirmResponse(true))
                    .await;
            }
            RallyEvent::FixPostConfirmNeeded(info) => {
                eprintln!(
                    "  [Post fix] {} (files: {})",
                    info.summary,
                    info.files_modified.join(", ")
                );
                eprintln!("  -> Auto-approving post (headless mode)");
                let _ = cmd_tx
                    .send(OrchestratorCommand::PostConfirmResponse(true))
                    .await;
            }
        }
    }

    // Channel closed - orchestrator finished without explicit terminal event
    HeadlessOutcome {
        result: HeadlessResult::NotApproved(
            last_error.unwrap_or_else(|| "Rally ended unexpectedly".to_string()),
        ),
        iterations: current_iteration,
        last_review,
        last_fix,
    }
}

/// Format ReviewerOutput as human-readable text (no JSON).
pub fn format_review_output(output: &ReviewerOutput) -> String {
    let mut lines = Vec::new();

    let action_str = match output.action {
        ReviewAction::Approve => "approve",
        ReviewAction::RequestChanges => "request_changes",
        ReviewAction::Comment => "comment",
    };
    lines.push(format!("[Review] Action: {}", action_str));
    lines.push(format!("  Summary: {}", output.summary));

    if !output.comments.is_empty() {
        lines.push(format!("  Comments ({}):", output.comments.len()));
        for comment in &output.comments {
            let severity = match comment.severity {
                CommentSeverity::Critical => "critical",
                CommentSeverity::Major => "major",
                CommentSeverity::Minor => "minor",
                CommentSeverity::Suggestion => "suggestion",
            };
            let location = format!("{}:{}", comment.path, comment.line);
            lines.push(format!("    - {} [{}] {}", location, severity, comment.body));
        }
    }

    if !output.blocking_issues.is_empty() {
        lines.push("  Blocking issues:".to_string());
        for issue in &output.blocking_issues {
            lines.push(format!("    - {}", issue));
        }
    }

    lines.join("\n")
}

/// Format RevieweeOutput as human-readable text (no JSON).
pub fn format_fix_output(output: &RevieweeOutput) -> String {
    let mut lines = Vec::new();

    let status_str = match output.status {
        RevieweeStatus::Completed => "completed",
        RevieweeStatus::NeedsClarification => "needs_clarification",
        RevieweeStatus::NeedsPermission => "needs_permission",
        RevieweeStatus::Error => "error",
    };
    lines.push(format!("[Fix] Status: {}", status_str));
    lines.push(format!("  Summary: {}", output.summary));

    if !output.files_modified.is_empty() {
        lines.push("  Files modified:".to_string());
        for file in &output.files_modified {
            lines.push(format!("    - {}", file));
        }
    }

    if let Some(error) = &output.error_details {
        lines.push(format!("  Error: {}", error));
    }

    lines.join("\n")
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        return s.to_string();
    }
    // Find byte position at the max_chars-th character boundary
    let byte_end = s
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    format!("{}...", &s[..byte_end])
}

/// Build JSON output from headless outcome (pure function for testability).
fn build_json_output(outcome: &HeadlessOutcome) -> HeadlessJsonOutput {
    let (result_kind, summary) = match &outcome.result {
        HeadlessResult::Approved(summary) => (HeadlessResultKind::Approved, summary.clone()),
        HeadlessResult::NotApproved(reason) => (HeadlessResultKind::NotApproved, reason.clone()),
        HeadlessResult::Error(msg) => (HeadlessResultKind::Error, msg.clone()),
    };
    HeadlessJsonOutput {
        result: result_kind,
        iterations: outcome.iterations,
        summary,
        last_review: outcome.last_review.clone(),
        last_fix: outcome.last_fix.clone(),
    }
}

/// Write JSON output to stdout with flush guarantee and broken pipe safety.
fn write_json_stdout(output: &HeadlessJsonOutput) {
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    match serde_json::to_writer(&mut handle, output) {
        Ok(()) => {
            let _ = writeln!(handle);
            let _ = handle.flush();
        }
        Err(e) => {
            eprintln!("[Headless] JSON serialization failed: {}", e);
        }
    }
}

/// Write error JSON to stdout for early failures (before event loop starts).
pub fn write_error_json(error: &str) {
    let output = HeadlessJsonOutput {
        result: HeadlessResultKind::Error,
        iterations: 0,
        summary: error.to_string(),
        last_review: None,
        last_fix: None,
    };
    write_json_stdout(&output);
}

/// Collect diff output for untracked files (mirrors loader.rs behavior).
///
/// Uses `git ls-files --others --exclude-standard` to discover untracked files,
/// then `git diff --no-index -- /dev/null <file>` to generate unified diffs.
async fn collect_untracked_diff(dir: &str) -> String {
    // List untracked files (respecting .gitignore)
    let ls_output = tokio::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(dir)
        .output()
        .await;

    let untracked_files: Vec<String> = match ls_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        }
        _ => return String::new(),
    };

    if untracked_files.is_empty() {
        return String::new();
    }

    eprintln!(
        "[Headless] Including {} untracked file(s) in diff",
        untracked_files.len()
    );

    let mut parts = Vec::new();
    for filename in &untracked_files {
        let diff_output = tokio::process::Command::new("git")
            .args([
                "diff",
                "--no-ext-diff",
                "--no-color",
                "--no-index",
                "--",
                "/dev/null",
                filename,
            ])
            .current_dir(dir)
            .output()
            .await;

        if let Ok(output) = diff_output {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if !stdout.trim().is_empty() {
                parts.push(stdout);
            }
        }
    }

    parts.join("\n")
}

/// Detect base branch for local diff (same logic as app.rs).
fn detect_local_base_branch(working_dir: Option<&str>) -> Option<String> {
    let dir = working_dir.unwrap_or(".");
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;

    if output.status.success() {
        let ref_str = String::from_utf8_lossy(&output.stdout);
        ref_str
            .trim()
            .strip_prefix("refs/remotes/origin/")
            .map(|s| s.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::{CommentSeverity, ReviewAction, ReviewComment, RevieweeStatus};

    #[test]
    fn test_format_review_output_approve() {
        let output = ReviewerOutput {
            action: ReviewAction::Approve,
            summary: "All looks good".to_string(),
            comments: vec![],
            blocking_issues: vec![],
        };

        let text = format_review_output(&output);
        assert!(text.contains("[Review] Action: approve"));
        assert!(text.contains("Summary: All looks good"));
        assert!(!text.contains("{"));
        assert!(!text.contains("}"));
    }

    #[test]
    fn test_format_review_output_request_changes() {
        let output = ReviewerOutput {
            action: ReviewAction::RequestChanges,
            summary: "Found 2 issues".to_string(),
            comments: vec![
                ReviewComment {
                    path: "src/main.rs".to_string(),
                    line: 42,
                    body: "Variable should be constant".to_string(),
                    severity: CommentSeverity::Major,
                },
                ReviewComment {
                    path: "src/lib.rs".to_string(),
                    line: 10,
                    body: "Consider renaming".to_string(),
                    severity: CommentSeverity::Minor,
                },
            ],
            blocking_issues: vec!["Error handling missing".to_string()],
        };

        let text = format_review_output(&output);
        assert!(text.contains("[Review] Action: request_changes"));
        assert!(text.contains("Comments (2):"));
        assert!(text.contains("src/main.rs:42 [major]"));
        assert!(text.contains("src/lib.rs:10 [minor]"));
        assert!(text.contains("Blocking issues:"));
        assert!(text.contains("Error handling missing"));
        // No JSON artifacts
        assert!(!text.contains("\"action\""));
        assert!(!text.contains("\"summary\""));
    }

    #[test]
    fn test_format_review_output_suggestion_severity() {
        let output = ReviewerOutput {
            action: ReviewAction::Comment,
            summary: "General feedback".to_string(),
            comments: vec![ReviewComment {
                path: "README.md".to_string(),
                line: 1,
                body: "Update docs".to_string(),
                severity: CommentSeverity::Suggestion,
            }],
            blocking_issues: vec![],
        };

        let text = format_review_output(&output);
        assert!(text.contains("README.md:1 [suggestion] Update docs"));
    }

    #[test]
    fn test_format_fix_output_completed() {
        let output = RevieweeOutput {
            status: RevieweeStatus::Completed,
            summary: "Fixed all issues".to_string(),
            files_modified: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            question: None,
            permission_request: None,
            error_details: None,
        };

        let text = format_fix_output(&output);
        assert!(text.contains("[Fix] Status: completed"));
        assert!(text.contains("Summary: Fixed all issues"));
        assert!(text.contains("Files modified:"));
        assert!(text.contains("- src/main.rs"));
        assert!(text.contains("- src/lib.rs"));
        // No JSON artifacts
        assert!(!text.contains("\"status\""));
        assert!(!text.contains("\"files_modified\""));
    }

    #[test]
    fn test_format_fix_output_error() {
        let output = RevieweeOutput {
            status: RevieweeStatus::Error,
            summary: "Build failed".to_string(),
            files_modified: vec![],
            question: None,
            permission_request: None,
            error_details: Some("cargo build exited with code 1".to_string()),
        };

        let text = format_fix_output(&output);
        assert!(text.contains("[Fix] Status: error"));
        assert!(text.contains("Error: cargo build exited with code 1"));
    }

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let long = "a".repeat(300);
        let result = truncate_str(&long, 200);
        assert!(result.ends_with("..."));
        // 200 ASCII chars + "..."
        assert_eq!(result.chars().count(), 203);
    }

    #[test]
    fn test_truncate_str_multibyte() {
        // Each Japanese char is 3 bytes in UTF-8
        let s = "あいうえおかきくけこ"; // 10 chars, 30 bytes
        let result = truncate_str(s, 5);
        assert_eq!(result, "あいうえお...");
        // Must not panic on multi-byte boundary
    }

    #[test]
    fn test_truncate_str_multibyte_mixed() {
        // Mixed ASCII and multibyte: "aあbいc" = 5 chars, 9 bytes
        let s = "aあbいc";
        assert_eq!(truncate_str(s, 3), "aあb...");
        assert_eq!(truncate_str(s, 5), "aあbいc");
        assert_eq!(truncate_str(s, 6), "aあbいc"); // longer than string
    }

    #[test]
    fn test_truncate_str_emoji() {
        // Emoji can be 4 bytes in UTF-8
        let s = "🎉🎊🎈🎁🎂";
        let result = truncate_str(s, 3);
        assert_eq!(result, "🎉🎊🎈...");
        assert_eq!(truncate_str(s, 5), "🎉🎊🎈🎁🎂");
    }

    #[test]
    fn test_truncate_str_exact_boundary() {
        let s = "abcde";
        assert_eq!(truncate_str(s, 5), "abcde");
        assert_eq!(truncate_str(s, 4), "abcd...");
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
        assert_eq!(truncate_str("", 0), "");
    }

    #[test]
    fn test_truncate_str_zero_max() {
        assert_eq!(truncate_str("hello", 0), "...");
    }

    #[test]
    fn test_json_output_approved() {
        let outcome = HeadlessOutcome {
            result: HeadlessResult::Approved("All good, no issues found".to_string()),
            iterations: 2,
            last_review: Some(ReviewerOutput {
                action: ReviewAction::Approve,
                summary: "All good".to_string(),
                comments: vec![],
                blocking_issues: vec![],
            }),
            last_fix: Some(RevieweeOutput {
                status: RevieweeStatus::Completed,
                summary: "Fixed".to_string(),
                files_modified: vec!["src/main.rs".to_string()],
                question: None,
                permission_request: None,
                error_details: None,
            }),
        };
        insta::assert_json_snapshot!(build_json_output(&outcome));
    }

    #[test]
    fn test_json_output_not_approved() {
        let outcome = HeadlessOutcome {
            result: HeadlessResult::NotApproved("Max iterations reached".to_string()),
            iterations: 3,
            last_review: Some(ReviewerOutput {
                action: ReviewAction::RequestChanges,
                summary: "Still has issues".to_string(),
                comments: vec![ReviewComment {
                    path: "src/lib.rs".to_string(),
                    line: 10,
                    body: "Fix this".to_string(),
                    severity: CommentSeverity::Major,
                }],
                blocking_issues: vec!["Error handling".to_string()],
            }),
            last_fix: Some(RevieweeOutput {
                status: RevieweeStatus::Completed,
                summary: "Attempted fix".to_string(),
                files_modified: vec!["src/lib.rs".to_string()],
                question: None,
                permission_request: None,
                error_details: None,
            }),
        };
        insta::assert_json_snapshot!(build_json_output(&outcome));
    }

    #[test]
    fn test_json_output_error_no_review() {
        let outcome = HeadlessOutcome {
            result: HeadlessResult::Error("Agent crashed".to_string()),
            iterations: 0,
            last_review: None,
            last_fix: None,
        };
        let output = build_json_output(&outcome);
        // None fields should be omitted via skip_serializing_if
        let json = serde_json::to_value(&output).unwrap();
        assert!(!json.as_object().unwrap().contains_key("last_review"));
        assert!(!json.as_object().unwrap().contains_key("last_fix"));
        insta::assert_json_snapshot!(output);
    }
}
