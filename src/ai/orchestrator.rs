use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::warn;

use crate::config::AiConfig;
use crate::github;
use crate::github::comment::{fetch_discussion_comments, fetch_review_comments};

use super::adapter::{
    AgentAdapter, Context, ExternalComment, ReviewAction, RevieweeOutput, RevieweeStatus,
    ReviewerOutput,
};
use super::adapters::create_adapter;
use super::prompt_loader::PromptLoader;
use super::prompts::{build_clarification_prompt, build_permission_granted_prompt};
use super::session::{write_history_entry, write_session, HistoryEntryType, RallySession};

/// Bot suffixes to identify bot users
const BOT_SUFFIXES: &[&str] = &["[bot]"];
/// Exact bot user names
const BOT_EXACT_MATCHES: &[&str] = &["github-actions", "dependabot"];
/// Maximum number of external comments to include in context
const MAX_EXTERNAL_COMMENTS: usize = 20;

/// Rally state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RallyState {
    Initializing,
    ReviewerReviewing,
    RevieweeFix,
    WaitingForClarification,
    WaitingForPermission,
    Completed,
    Aborted,
    Error,
}

impl RallyState {
    /// Rally が実行中（完了・エラー・中断以外）かどうか
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        !matches!(
            self,
            RallyState::Completed | RallyState::Aborted | RallyState::Error
        )
    }

    /// Rally が完了、中断、またはエラーで終了したかどうか
    #[allow(dead_code)]
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            RallyState::Completed | RallyState::Aborted | RallyState::Error
        )
    }
}

/// Event emitted during rally for TUI updates
///
/// Variants are used by TUI handlers (ui/ai_rally.rs) via mpsc channel
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RallyEvent {
    StateChanged(RallyState),
    IterationStarted(u32),
    ReviewCompleted(ReviewerOutput),
    FixCompleted(RevieweeOutput),
    ClarificationNeeded(String),
    PermissionNeeded(String, String), // action, reason
    Approved(String),                 // summary
    Error(String),
    Log(String),
    // Streaming events from Claude
    AgentThinking(String),           // thinking content
    AgentToolUse(String, String),    // tool_name, input_summary
    AgentToolResult(String, String), // tool_name, result_summary
    AgentText(String),               // text output
}

/// Result of the rally process
///
/// Used by app.rs to handle rally completion state
#[derive(Debug)]
#[allow(dead_code)]
pub enum RallyResult {
    Approved { iteration: u32, summary: String },
    MaxIterationsReached { iteration: u32 },
    Aborted { iteration: u32, reason: String },
    Error { iteration: u32, error: String },
}

/// Command sent from TUI to Orchestrator
#[derive(Debug)]
pub enum OrchestratorCommand {
    /// User provided clarification answer
    ClarificationResponse(String),
    /// User granted or denied permission
    PermissionResponse(bool),
    /// User requested abort
    Abort,
}

/// Main orchestrator for AI rally
pub struct Orchestrator {
    repo: String,
    pr_number: u32,
    config: AiConfig,
    reviewer_adapter: Box<dyn AgentAdapter>,
    reviewee_adapter: Box<dyn AgentAdapter>,
    session: RallySession,
    context: Option<Context>,
    last_review: Option<ReviewerOutput>,
    last_fix: Option<RevieweeOutput>,
    event_sender: mpsc::Sender<RallyEvent>,
    prompt_loader: PromptLoader,
    /// Command receiver for TUI commands
    command_receiver: Option<mpsc::Receiver<OrchestratorCommand>>,
}

impl Orchestrator {
    pub fn new(
        repo: &str,
        pr_number: u32,
        config: AiConfig,
        event_sender: mpsc::Sender<RallyEvent>,
        command_receiver: Option<mpsc::Receiver<OrchestratorCommand>>,
    ) -> Result<Self> {
        let mut reviewer_adapter = create_adapter(&config.reviewer, &config)?;
        let mut reviewee_adapter = create_adapter(&config.reviewee, &config)?;

        // Set event sender for streaming events
        reviewer_adapter.set_event_sender(event_sender.clone());
        reviewee_adapter.set_event_sender(event_sender.clone());

        let session = RallySession::new(repo, pr_number);
        let prompt_loader = PromptLoader::new(&config);

        Ok(Self {
            repo: repo.to_string(),
            pr_number,
            config,
            reviewer_adapter,
            reviewee_adapter,
            session,
            context: None,
            last_review: None,
            last_fix: None,
            event_sender,
            prompt_loader,
            command_receiver,
        })
    }

    /// Set the context for the rally
    pub fn set_context(&mut self, context: Context) {
        self.context = Some(context);
    }

    /// Run the rally process
    pub async fn run(&mut self) -> Result<RallyResult> {
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| anyhow!("Context not set"))?
            .clone();

        self.send_event(RallyEvent::StateChanged(RallyState::Initializing))
            .await;

        // Main loop
        while self.session.iteration < self.config.max_iterations {
            self.session.increment_iteration();
            let iteration = self.session.iteration;

            self.send_event(RallyEvent::IterationStarted(iteration))
                .await;

            // Update head_sha at start of each iteration.
            // Note: The reviewee does NOT push changes; commits are local only.
            // This update is primarily for when the user manually pushes changes between iterations,
            // or when external tools/CI update the PR branch.
            if iteration > 1 {
                if let Err(e) = self.update_head_sha().await {
                    warn!("Failed to update head_sha: {}", e);
                }
            }

            // Run reviewer
            self.session.update_state(RallyState::ReviewerReviewing);
            self.send_event(RallyEvent::StateChanged(RallyState::ReviewerReviewing))
                .await;
            write_session(&self.session)?;

            let review_result = self.run_reviewer_with_timeout(&context, iteration).await?;

            // Store the review for later use
            write_history_entry(
                &self.repo,
                self.pr_number,
                iteration,
                &HistoryEntryType::Review(review_result.clone()),
            )?;

            self.send_event(RallyEvent::ReviewCompleted(review_result.clone()))
                .await;
            self.last_review = Some(review_result.clone());

            // Update head_sha before posting review (ensure we have the latest commit)
            if let Err(e) = self.update_head_sha().await {
                warn!("Failed to update head_sha before posting review: {}", e);
            }

            // Post review to PR
            if let Err(e) = self.post_review_to_pr(&review_result).await {
                warn!("Failed to post review to PR: {}", e);
                self.send_event(RallyEvent::Log(format!(
                    "Warning: Failed to post review to PR: {}",
                    e
                )))
                .await;
            }

            // Check for approval
            if review_result.action == ReviewAction::Approve {
                self.session.update_state(RallyState::Completed);
                write_session(&self.session)?;

                self.send_event(RallyEvent::Approved(review_result.summary.clone()))
                    .await;
                self.send_event(RallyEvent::StateChanged(RallyState::Completed))
                    .await;

                return Ok(RallyResult::Approved {
                    iteration,
                    summary: review_result.summary,
                });
            }

            // Run reviewee to fix issues
            self.session.update_state(RallyState::RevieweeFix);
            self.send_event(RallyEvent::StateChanged(RallyState::RevieweeFix))
                .await;
            write_session(&self.session)?;

            // Fetch external comments before reviewee starts
            let external_comments = self.fetch_external_comments().await;
            if !external_comments.is_empty() {
                self.send_event(RallyEvent::Log(format!(
                    "Fetched {} external bot comments",
                    external_comments.len()
                )))
                .await;
            }
            if let Some(ref mut ctx) = self.context {
                ctx.external_comments = external_comments;
            }

            // Get updated context with external comments
            let context = self
                .context
                .as_ref()
                .ok_or_else(|| anyhow!("Context not set"))?
                .clone();

            let fix_result = self
                .run_reviewee_with_timeout(&context, &review_result, iteration)
                .await?;

            write_history_entry(
                &self.repo,
                self.pr_number,
                iteration,
                &HistoryEntryType::Fix(fix_result.clone()),
            )?;

            self.send_event(RallyEvent::FixCompleted(fix_result.clone()))
                .await;

            // Handle reviewee status
            match fix_result.status {
                RevieweeStatus::Completed => {
                    // Store the fix result for the next re-review
                    self.last_fix = Some(fix_result.clone());

                    // Post fix summary to PR
                    if let Err(e) = self.post_fix_comment(&fix_result).await {
                        warn!("Failed to post fix comment to PR: {}", e);
                        self.send_event(RallyEvent::Log(format!(
                            "Warning: Failed to post fix comment to PR: {}",
                            e
                        )))
                        .await;
                    }

                    // Continue to next iteration
                }
                RevieweeStatus::NeedsClarification => {
                    if let Some(question) = &fix_result.question {
                        self.session
                            .update_state(RallyState::WaitingForClarification);
                        write_session(&self.session)?;

                        self.send_event(RallyEvent::ClarificationNeeded(question.clone()))
                            .await;
                        self.send_event(RallyEvent::StateChanged(
                            RallyState::WaitingForClarification,
                        ))
                        .await;

                        // Wait for user command
                        match self.wait_for_command().await {
                            Some(OrchestratorCommand::ClarificationResponse(answer)) => {
                                // Handle clarification response
                                if let Err(e) = self.handle_clarification_response(&answer).await {
                                    self.session.update_state(RallyState::Error);
                                    write_session(&self.session)?;
                                    self.send_event(RallyEvent::Error(e.to_string())).await;
                                    self.send_event(RallyEvent::StateChanged(RallyState::Error))
                                        .await;
                                    return Ok(RallyResult::Error {
                                        iteration,
                                        error: e.to_string(),
                                    });
                                }
                                // Continue to next iteration
                            }
                            Some(OrchestratorCommand::Abort) | None => {
                                let reason = format!("Clarification aborted: {}", question);
                                self.session.update_state(RallyState::Aborted);
                                write_session(&self.session)?;
                                self.send_event(RallyEvent::Log(reason.clone())).await;
                                self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                                    .await;
                                return Ok(RallyResult::Aborted { iteration, reason });
                            }
                            Some(OrchestratorCommand::PermissionResponse(_)) => {
                                // Ignore invalid command
                                let reason = format!("Clarification needed: {}", question);
                                self.session.update_state(RallyState::Aborted);
                                write_session(&self.session)?;
                                self.send_event(RallyEvent::Log(reason.clone())).await;
                                self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                                    .await;
                                return Ok(RallyResult::Aborted { iteration, reason });
                            }
                        }
                    }
                }
                RevieweeStatus::NeedsPermission => {
                    if let Some(perm) = &fix_result.permission_request {
                        self.session.update_state(RallyState::WaitingForPermission);
                        write_session(&self.session)?;

                        self.send_event(RallyEvent::PermissionNeeded(
                            perm.action.clone(),
                            perm.reason.clone(),
                        ))
                        .await;
                        self.send_event(RallyEvent::StateChanged(RallyState::WaitingForPermission))
                            .await;

                        // Wait for user command
                        match self.wait_for_command().await {
                            Some(OrchestratorCommand::PermissionResponse(approved)) => {
                                if approved {
                                    // Handle permission granted
                                    if let Err(e) =
                                        self.handle_permission_granted(&perm.action).await
                                    {
                                        self.session.update_state(RallyState::Error);
                                        write_session(&self.session)?;
                                        self.send_event(RallyEvent::Error(e.to_string())).await;
                                        self.send_event(RallyEvent::StateChanged(
                                            RallyState::Error,
                                        ))
                                        .await;
                                        return Ok(RallyResult::Error {
                                            iteration,
                                            error: e.to_string(),
                                        });
                                    }
                                    // Continue to next iteration
                                } else {
                                    // Permission denied
                                    let reason = format!("Permission denied: {}", perm.action);
                                    self.session.update_state(RallyState::Aborted);
                                    write_session(&self.session)?;
                                    self.send_event(RallyEvent::Log(reason.clone())).await;
                                    self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                                        .await;
                                    return Ok(RallyResult::Aborted { iteration, reason });
                                }
                            }
                            Some(OrchestratorCommand::Abort) | None => {
                                let reason = format!("Permission aborted: {}", perm.action);
                                self.session.update_state(RallyState::Aborted);
                                write_session(&self.session)?;
                                self.send_event(RallyEvent::Log(reason.clone())).await;
                                self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                                    .await;
                                return Ok(RallyResult::Aborted { iteration, reason });
                            }
                            Some(OrchestratorCommand::ClarificationResponse(_)) => {
                                // Ignore invalid command
                                let reason = format!("Permission needed: {}", perm.action);
                                self.session.update_state(RallyState::Aborted);
                                write_session(&self.session)?;
                                self.send_event(RallyEvent::Log(reason.clone())).await;
                                self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                                    .await;
                                return Ok(RallyResult::Aborted { iteration, reason });
                            }
                        }
                    }
                }
                RevieweeStatus::Error => {
                    self.session.update_state(RallyState::Error);
                    write_session(&self.session)?;

                    let error = fix_result
                        .error_details
                        .unwrap_or_else(|| "Unknown error".to_string());
                    self.send_event(RallyEvent::Error(error.clone())).await;
                    self.send_event(RallyEvent::StateChanged(RallyState::Error))
                        .await;

                    return Ok(RallyResult::Error { iteration, error });
                }
            }
        }

        self.send_event(RallyEvent::Log(format!(
            "Max iterations ({}) reached",
            self.config.max_iterations
        )))
        .await;

        Ok(RallyResult::MaxIterationsReached {
            iteration: self.session.iteration,
        })
    }

    /// Wait for a command from the TUI
    async fn wait_for_command(&mut self) -> Option<OrchestratorCommand> {
        let rx = self.command_receiver.as_mut()?;
        rx.recv().await
    }

    /// Handle clarification response from user
    async fn handle_clarification_response(&mut self, answer: &str) -> Result<()> {
        self.send_event(RallyEvent::Log(format!(
            "User provided clarification: {}",
            answer
        )))
        .await;

        // Ask reviewer for clarification and log the response
        let prompt = build_clarification_prompt(answer);
        let reviewer_response = self.reviewer_adapter.continue_reviewer(&prompt).await?;

        // Log the reviewer's response for debugging/audit purposes
        self.send_event(RallyEvent::Log(format!(
            "Reviewer clarification response: {}",
            reviewer_response.summary
        )))
        .await;

        // Continue reviewee with the answer
        self.reviewee_adapter.continue_reviewee(answer).await?;

        self.session.update_state(RallyState::RevieweeFix);
        self.send_event(RallyEvent::StateChanged(RallyState::RevieweeFix))
            .await;
        write_session(&self.session)?;

        Ok(())
    }

    /// Handle permission granted from user
    async fn handle_permission_granted(&mut self, action: &str) -> Result<()> {
        self.send_event(RallyEvent::Log(format!(
            "User granted permission for: {}",
            action
        )))
        .await;

        let prompt = build_permission_granted_prompt(action);
        self.reviewee_adapter.continue_reviewee(&prompt).await?;

        self.session.update_state(RallyState::RevieweeFix);
        self.send_event(RallyEvent::StateChanged(RallyState::RevieweeFix))
            .await;
        write_session(&self.session)?;

        Ok(())
    }

    /// Continue after clarification answer (legacy, kept for compatibility)
    #[allow(dead_code)]
    pub async fn continue_with_clarification(&mut self, answer: &str) -> Result<()> {
        self.handle_clarification_response(answer).await
    }

    /// Continue after permission granted (legacy, kept for compatibility)
    #[allow(dead_code)]
    pub async fn continue_with_permission(&mut self, action: &str) -> Result<()> {
        self.handle_permission_granted(action).await
    }

    async fn run_reviewer_with_timeout(
        &mut self,
        context: &Context,
        iteration: u32,
    ) -> Result<ReviewerOutput> {
        let prompt = if iteration == 1 {
            self.prompt_loader.load_reviewer_prompt(context, iteration)
        } else {
            // Re-review after fixes - fetch updated diff and include fix summary
            let updated_diff = self.fetch_current_diff().await.unwrap_or_else(|e| {
                warn!("Failed to fetch updated diff: {}", e);
                context.diff.clone()
            });

            let changes_summary = self
                .last_fix
                .as_ref()
                .map(|f| {
                    let files = if f.files_modified.is_empty() {
                        "No files modified".to_string()
                    } else {
                        f.files_modified.join(", ")
                    };
                    format!("{}\n\nFiles modified: {}", f.summary, files)
                })
                .unwrap_or_else(|| "No changes recorded".to_string());
            self.prompt_loader.load_rereview_prompt(
                context,
                iteration,
                &changes_summary,
                &updated_diff,
            )
        };

        let duration = Duration::from_secs(self.config.timeout_secs);

        timeout(
            duration,
            self.reviewer_adapter.run_reviewer(&prompt, context),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "Reviewer timeout after {} seconds",
                self.config.timeout_secs
            )
        })?
    }

    async fn run_reviewee_with_timeout(
        &mut self,
        context: &Context,
        review: &ReviewerOutput,
        iteration: u32,
    ) -> Result<RevieweeOutput> {
        let prompt = self
            .prompt_loader
            .load_reviewee_prompt(context, review, iteration);
        let duration = Duration::from_secs(self.config.timeout_secs);

        timeout(
            duration,
            self.reviewee_adapter.run_reviewee(&prompt, context),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "Reviewee timeout after {} seconds",
                self.config.timeout_secs
            )
        })?
    }

    async fn send_event(&self, event: RallyEvent) {
        let _ = self.event_sender.send(event).await;
    }

    /// Post review to PR (summary comment + inline comments)
    async fn post_review_to_pr(&self, review: &ReviewerOutput) -> Result<()> {
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| anyhow!("Context not set"))?;

        // Map AI ReviewAction to App ReviewAction
        let app_action = match review.action {
            ReviewAction::Approve => crate::app::ReviewAction::Approve,
            ReviewAction::RequestChanges => crate::app::ReviewAction::RequestChanges,
            ReviewAction::Comment => crate::app::ReviewAction::Comment,
        };

        // Copy for potential fallback use (app_action is moved into submit_review)
        let app_action_for_fallback = app_action;

        // Add prefix to summary
        let summary_with_prefix = format!("[AI Rally - Reviewer]\n\n{}", review.summary);

        // Post summary comment using gh pr review
        // If approve fails (e.g., can't approve own PR), fall back to comment
        let result =
            github::submit_review(&self.repo, self.pr_number, app_action, &summary_with_prefix)
                .await;

        if result.is_err() && matches!(app_action_for_fallback, crate::app::ReviewAction::Approve) {
            warn!("Approve failed, falling back to comment");
            github::submit_review(
                &self.repo,
                self.pr_number,
                crate::app::ReviewAction::Comment,
                &summary_with_prefix,
            )
            .await?;
        } else {
            result?;
        }

        // Post inline comments with rate limit handling
        for comment in &review.comments {
            // Add prefix to inline comment
            let body_with_prefix = format!("[AI Rally - Reviewer]\n\n{}", comment.body);
            if let Err(e) = github::create_review_comment(
                &self.repo,
                self.pr_number,
                &context.head_sha,
                &comment.path,
                comment.line,
                &body_with_prefix,
            )
            .await
            {
                warn!(
                    "Failed to post inline comment on {}:{}: {}",
                    comment.path, comment.line, e
                );
            }
            // Rate limit mitigation: small delay between API calls
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    /// Post fix summary comment to PR
    async fn post_fix_comment(&self, fix: &RevieweeOutput) -> Result<()> {
        // Build comment body with files modified
        let files_list = if fix.files_modified.is_empty() {
            "No files modified".to_string()
        } else {
            fix.files_modified
                .iter()
                .map(|f| format!("- `{}`", f))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let comment_body = format!(
            "[AI Rally - Reviewee]\n\n{}\n\n**Files modified:**\n{}",
            fix.summary, files_list
        );

        // Post as a comment (not a review)
        github::submit_review(
            &self.repo,
            self.pr_number,
            crate::app::ReviewAction::Comment,
            &comment_body,
        )
        .await?;

        Ok(())
    }

    /// Fetch external comments from bots (Copilot, CodeRabbit, etc.)
    async fn fetch_external_comments(&self) -> Vec<ExternalComment> {
        let mut comments = Vec::new();

        // Fetch review comments (inline comments on diff)
        if let Ok(review_comments) = fetch_review_comments(&self.repo, self.pr_number).await {
            for c in review_comments {
                if is_bot_user(&c.user.login) {
                    comments.push(ExternalComment {
                        source: c.user.login.clone(),
                        path: Some(c.path.clone()),
                        line: c.line,
                        body: c.body.clone(),
                    });
                }
            }
        }

        // Fetch discussion comments (general PR comments)
        if let Ok(discussion) = fetch_discussion_comments(&self.repo, self.pr_number).await {
            for c in discussion {
                if is_bot_user(&c.user.login) {
                    comments.push(ExternalComment {
                        source: c.user.login.clone(),
                        path: None,
                        line: None,
                        body: c.body.clone(),
                    });
                }
            }
        }

        // Limit the number of comments
        comments.truncate(MAX_EXTERNAL_COMMENTS);
        comments
    }

    /// Update head_sha from PR
    ///
    /// Note: The reviewee does NOT push changes; commits are local only.
    /// This update is for when the user manually pushes between iterations,
    /// or when external tools/CI update the PR branch.
    async fn update_head_sha(&mut self) -> Result<()> {
        let pr = github::fetch_pr(&self.repo, self.pr_number).await?;
        if let Some(ref mut ctx) = self.context {
            ctx.head_sha = pr.head.sha.clone();
        }
        Ok(())
    }

    /// Fetch current diff from GitHub API
    async fn fetch_current_diff(&self) -> Result<String> {
        github::fetch_pr_diff(&self.repo, self.pr_number).await
    }

    // For debugging and session inspection
    #[allow(dead_code)]
    pub fn session(&self) -> &RallySession {
        &self.session
    }
}

/// Check if a user is a bot
fn is_bot_user(login: &str) -> bool {
    BOT_SUFFIXES.iter().any(|suffix| login.ends_with(suffix)) || BOT_EXACT_MATCHES.contains(&login)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_orchestrator_command_variants() {
        // Test ClarificationResponse
        let cmd = OrchestratorCommand::ClarificationResponse("test answer".to_string());
        match cmd {
            OrchestratorCommand::ClarificationResponse(answer) => {
                assert_eq!(answer, "test answer");
            }
            _ => panic!("Expected ClarificationResponse"),
        }

        // Test PermissionResponse approved
        let cmd = OrchestratorCommand::PermissionResponse(true);
        match cmd {
            OrchestratorCommand::PermissionResponse(approved) => {
                assert!(approved);
            }
            _ => panic!("Expected PermissionResponse"),
        }

        // Test PermissionResponse denied
        let cmd = OrchestratorCommand::PermissionResponse(false);
        match cmd {
            OrchestratorCommand::PermissionResponse(approved) => {
                assert!(!approved);
            }
            _ => panic!("Expected PermissionResponse"),
        }

        // Test Abort
        let cmd = OrchestratorCommand::Abort;
        assert!(matches!(cmd, OrchestratorCommand::Abort));
    }

    #[tokio::test]
    async fn test_command_channel_clarification() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        // Send clarification response
        tx.send(OrchestratorCommand::ClarificationResponse(
            "user's answer".to_string(),
        ))
        .await
        .unwrap();

        // Receive and verify
        let cmd = rx.recv().await.unwrap();
        match cmd {
            OrchestratorCommand::ClarificationResponse(answer) => {
                assert_eq!(answer, "user's answer");
            }
            _ => panic!("Expected ClarificationResponse"),
        }
    }

    #[tokio::test]
    async fn test_command_channel_permission_granted() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        tx.send(OrchestratorCommand::PermissionResponse(true))
            .await
            .unwrap();

        let cmd = rx.recv().await.unwrap();
        match cmd {
            OrchestratorCommand::PermissionResponse(approved) => {
                assert!(approved, "Permission should be granted");
            }
            _ => panic!("Expected PermissionResponse"),
        }
    }

    #[tokio::test]
    async fn test_command_channel_permission_denied() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        tx.send(OrchestratorCommand::PermissionResponse(false))
            .await
            .unwrap();

        let cmd = rx.recv().await.unwrap();
        match cmd {
            OrchestratorCommand::PermissionResponse(approved) => {
                assert!(!approved, "Permission should be denied");
            }
            _ => panic!("Expected PermissionResponse"),
        }
    }

    #[tokio::test]
    async fn test_command_channel_abort() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        tx.send(OrchestratorCommand::Abort).await.unwrap();

        let cmd = rx.recv().await.unwrap();
        assert!(matches!(cmd, OrchestratorCommand::Abort));
    }

    #[tokio::test]
    async fn test_command_channel_closed_returns_none() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        // Drop sender to close channel
        drop(tx);

        // Receive should return None
        let cmd = rx.recv().await;
        assert!(cmd.is_none());
    }

    #[test]
    fn test_is_bot_user() {
        // Bot suffixes
        assert!(is_bot_user("copilot[bot]"));
        assert!(is_bot_user("coderabbitai[bot]"));
        assert!(is_bot_user("renovate[bot]"));

        // Exact matches
        assert!(is_bot_user("github-actions"));
        assert!(is_bot_user("dependabot"));

        // Non-bot users
        assert!(!is_bot_user("ushironoko"));
        assert!(!is_bot_user("octocat"));
        assert!(!is_bot_user("bot")); // "bot" alone is not a bot suffix
    }

    #[test]
    fn test_rally_state_is_active() {
        assert!(RallyState::Initializing.is_active());
        assert!(RallyState::ReviewerReviewing.is_active());
        assert!(RallyState::RevieweeFix.is_active());
        assert!(RallyState::WaitingForClarification.is_active());
        assert!(RallyState::WaitingForPermission.is_active());
        assert!(!RallyState::Completed.is_active());
        assert!(!RallyState::Aborted.is_active());
        assert!(!RallyState::Error.is_active());
    }

    #[test]
    fn test_rally_state_is_finished() {
        assert!(!RallyState::Initializing.is_finished());
        assert!(!RallyState::ReviewerReviewing.is_finished());
        assert!(!RallyState::RevieweeFix.is_finished());
        assert!(!RallyState::WaitingForClarification.is_finished());
        assert!(!RallyState::WaitingForPermission.is_finished());
        assert!(RallyState::Completed.is_finished());
        assert!(RallyState::Aborted.is_finished());
        assert!(RallyState::Error.is_finished());
    }
}
