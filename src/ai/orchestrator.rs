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
use super::prompts::{
    build_clarification_prompt, build_clarification_skipped_prompt, build_permission_denied_prompt,
    build_permission_granted_prompt,
};
use super::session::{write_history_entry, write_session, HistoryEntryType, RallySession};

/// Bot suffixes to identify bot users
const BOT_SUFFIXES: &[&str] = &["[bot]"];
/// Exact bot user names
const BOT_EXACT_MATCHES: &[&str] = &["github-actions", "dependabot"];
/// Maximum number of external comments to include in context
const MAX_EXTERNAL_COMMENTS: usize = 20;

/// Git subcommands that are safe (read-only or local-only operations)
/// for the reviewee to execute. Any git subcommand not in this list
/// is blocked when validating permission requests in local mode.
const ALLOWED_GIT_SUBCOMMANDS: &[&str] = &[
    "status", "diff", "add", "commit", "log", "show", "branch", "switch", "stash",
];

/// Rally state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RallyState {
    Initializing,
    ReviewerReviewing,
    RevieweeFix,
    WaitingForClarification,
    WaitingForPermission,
    WaitingForPostConfirmation,
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
    ReviewPostConfirmNeeded(ReviewPostInfo),
    FixPostConfirmNeeded(FixPostInfo),
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

/// Lightweight DTO for review post confirmation (sent via RallyEvent)
#[derive(Debug, Clone)]
pub struct ReviewPostInfo {
    pub action: String,
    pub summary: String,
    pub comment_count: usize,
}

/// Lightweight DTO for fix post confirmation (sent via RallyEvent)
#[derive(Debug, Clone)]
pub struct FixPostInfo {
    pub summary: String,
    pub files_modified: Vec<String>,
}

/// Command sent from TUI to Orchestrator
#[derive(Debug)]
pub enum OrchestratorCommand {
    /// User provided clarification answer
    ClarificationResponse(String),
    /// User granted or denied permission
    PermissionResponse(bool),
    /// User chose to skip clarification (continue with best judgment)
    SkipClarification,
    /// User approved or skipped post confirmation
    PostConfirmResponse(bool),
    /// User requested abort (stop the rally entirely)
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
            if let Err(e) = write_session(&self.session) {
                warn!("Failed to write session: {}", e);
                self.send_event(RallyEvent::Log(format!(
                    "Warning: Failed to write session: {}",
                    e
                )))
                .await;
            }

            let review_result = match self.run_reviewer_with_timeout(&context, iteration).await {
                Ok(result) => result,
                Err(e) => {
                    self.session.update_state(RallyState::Error);
                    let _ = write_session(&self.session);
                    self.send_event(RallyEvent::Error(format!("Reviewer failed: {:#}", e)))
                        .await;
                    self.send_event(RallyEvent::StateChanged(RallyState::Error))
                        .await;
                    return Err(e);
                }
            };

            // Store the review for later use
            if let Err(e) = write_history_entry(
                &self.repo,
                self.pr_number,
                iteration,
                &HistoryEntryType::Review(review_result.clone()),
            ) {
                warn!("Failed to write review history: {}", e);
                self.send_event(RallyEvent::Log(format!(
                    "Warning: Failed to write review history: {}",
                    e
                )))
                .await;
            }

            self.send_event(RallyEvent::ReviewCompleted(review_result.clone()))
                .await;
            self.last_review = Some(review_result.clone());

            // Update head_sha before posting review (ensure we have the latest commit)
            if let Err(e) = self.update_head_sha().await {
                warn!("Failed to update head_sha before posting review: {}", e);
            }

            // Post review to PR (with confirmation if auto_post is false)
            if let Err(e) = self.maybe_post_review_to_pr(&review_result).await {
                // Check if abort was triggered during post confirmation
                if self.session.state == RallyState::Aborted {
                    return Ok(RallyResult::Aborted {
                        iteration,
                        reason: e.to_string(),
                    });
                }
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
                if let Err(e) = write_session(&self.session) {
                    warn!("Failed to write session: {}", e);
                }

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
            if let Err(e) = write_session(&self.session) {
                warn!("Failed to write session: {}", e);
                self.send_event(RallyEvent::Log(format!(
                    "Warning: Failed to write session: {}",
                    e
                )))
                .await;
            }

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

            let fix_result = match self
                .run_reviewee_with_timeout(&context, &review_result, iteration)
                .await
            {
                Ok(result) => result,
                Err(e) => {
                    self.session.update_state(RallyState::Error);
                    let _ = write_session(&self.session);
                    self.send_event(RallyEvent::Error(format!("Reviewee failed: {:#}", e)))
                        .await;
                    self.send_event(RallyEvent::StateChanged(RallyState::Error))
                        .await;
                    return Err(e);
                }
            };

            if let Err(e) = write_history_entry(
                &self.repo,
                self.pr_number,
                iteration,
                &HistoryEntryType::Fix(fix_result.clone()),
            ) {
                warn!("Failed to write fix history: {}", e);
                self.send_event(RallyEvent::Log(format!(
                    "Warning: Failed to write fix history: {}",
                    e
                )))
                .await;
            }

            self.send_event(RallyEvent::FixCompleted(fix_result.clone()))
                .await;

            // Handle reviewee status
            match fix_result.status {
                RevieweeStatus::Completed => {
                    // Store the fix result for the next re-review
                    self.last_fix = Some(fix_result.clone());

                    // Post fix summary to PR (with confirmation if auto_post is false)
                    if let Err(e) = self.maybe_post_fix_comment(&fix_result).await {
                        // Check if abort was triggered during post confirmation
                        if self.session.state == RallyState::Aborted {
                            return Ok(RallyResult::Aborted {
                                iteration,
                                reason: e.to_string(),
                            });
                        }
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
                        if let Err(e) = write_session(&self.session) {
                            warn!("Failed to write session: {}", e);
                        }

                        self.send_event(RallyEvent::ClarificationNeeded(question.clone()))
                            .await;
                        self.send_event(RallyEvent::StateChanged(
                            RallyState::WaitingForClarification,
                        ))
                        .await;

                        // Wait for user command (loop to skip stale/invalid commands)
                        loop {
                            match self.wait_for_command().await {
                                Some(OrchestratorCommand::ClarificationResponse(answer)) => {
                                    // Handle clarification response
                                    if let Err(e) = self.handle_clarification_response(&answer).await {
                                        self.session.update_state(RallyState::Error);
                                        let _ = write_session(&self.session);
                                        self.send_event(RallyEvent::Error(e.to_string())).await;
                                        self.send_event(RallyEvent::StateChanged(RallyState::Error))
                                            .await;
                                        return Ok(RallyResult::Error {
                                            iteration,
                                            error: e.to_string(),
                                        });
                                    }
                                    // Continue to next iteration
                                    break;
                                }
                                Some(OrchestratorCommand::SkipClarification) => {
                                    // Clarification skipped - continue with best judgment
                                    self.send_event(RallyEvent::Log(format!(
                                        "Clarification skipped for: {}. Continuing with best judgment...",
                                        question
                                    )))
                                    .await;

                                    let prompt = build_clarification_skipped_prompt(question);
                                    match self.reviewee_adapter.continue_reviewee(&prompt).await {
                                        Ok(output) => {
                                            // Write history entry for the follow-up fix
                                            if let Err(e) = write_history_entry(
                                                &self.repo,
                                                self.pr_number,
                                                iteration,
                                                &HistoryEntryType::Fix(output.clone()),
                                            ) {
                                                warn!("Failed to write follow-up fix history: {}", e);
                                            }

                                            // Post fix comment to PR (with confirmation if auto_post is false)
                                            if let Err(e) = self.maybe_post_fix_comment(&output).await {
                                                // Check if abort was triggered during post confirmation
                                                if self.session.state == RallyState::Aborted {
                                                    return Ok(RallyResult::Aborted {
                                                        iteration,
                                                        reason: e.to_string(),
                                                    });
                                                }
                                                warn!(
                                                    "Failed to post follow-up fix comment to PR: {}",
                                                    e
                                                );
                                            }

                                            self.send_event(RallyEvent::FixCompleted(output.clone()))
                                                .await;
                                            self.last_fix = Some(output);
                                        }
                                        Err(e) => {
                                            self.last_fix = None;
                                            self.send_event(RallyEvent::Log(format!(
                                                "Error continuing after clarification skip: {}. Proceeding to re-review.",
                                                e
                                            )))
                                            .await;
                                        }
                                    }

                                    // Notify TUI of state change
                                    self.session.update_state(RallyState::RevieweeFix);
                                    self.send_event(RallyEvent::StateChanged(RallyState::RevieweeFix))
                                        .await;
                                    let _ = write_session(&self.session);
                                    // Continue loop
                                    break;
                                }
                                Some(OrchestratorCommand::Abort) | None => {
                                    // True abort - user cancelled or channel closed
                                    let reason = "Clarification cancelled by user".to_string();
                                    self.session.update_state(RallyState::Aborted);
                                    let _ = write_session(&self.session);
                                    self.send_event(RallyEvent::Log(reason.clone())).await;
                                    self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                                        .await;
                                    return Ok(RallyResult::Aborted { iteration, reason });
                                }
                                _ => {
                                    // Stale/invalid command for this state (e.g. PostConfirmResponse) - ignore and re-wait
                                    warn!("Received invalid command during WaitingForClarification, ignoring");
                                    self.send_event(RallyEvent::Log(
                                        "Received invalid command, still waiting for clarification...".to_string(),
                                    ))
                                    .await;
                                    continue;
                                }
                            }
                        }
                    }
                }
                RevieweeStatus::NeedsPermission => {
                    if let Some(perm) = &fix_result.permission_request {
                        self.session.update_state(RallyState::WaitingForPermission);
                        let _ = write_session(&self.session);

                        self.send_event(RallyEvent::PermissionNeeded(
                            perm.action.clone(),
                            perm.reason.clone(),
                        ))
                        .await;
                        self.send_event(RallyEvent::StateChanged(RallyState::WaitingForPermission))
                            .await;

                        // Wait for user command (loop to skip stale/invalid commands)
                        loop {
                            match self.wait_for_command().await {
                                Some(OrchestratorCommand::PermissionResponse(approved)) => {
                                    if approved {
                                        // Handle permission granted
                                        if let Err(e) =
                                            self.handle_permission_granted(&perm.action).await
                                        {
                                            self.session.update_state(RallyState::Error);
                                            let _ = write_session(&self.session);
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
                                        // Permission denied - continue without this permission
                                        self.send_event(RallyEvent::Log(format!(
                                            "Permission denied for: {}. Continuing without it...",
                                            perm.action
                                        )))
                                        .await;

                                        let prompt =
                                            build_permission_denied_prompt(&perm.action, &perm.reason);
                                        match self.reviewee_adapter.continue_reviewee(&prompt).await {
                                            Ok(output) => {
                                                // Write history entry for the follow-up fix
                                                if let Err(e) = write_history_entry(
                                                    &self.repo,
                                                    self.pr_number,
                                                    iteration,
                                                    &HistoryEntryType::Fix(output.clone()),
                                                ) {
                                                    warn!(
                                                        "Failed to write follow-up fix history: {}",
                                                        e
                                                    );
                                                }

                                                // Post fix comment to PR (with confirmation if auto_post is false)
                                                if let Err(e) = self.maybe_post_fix_comment(&output).await {
                                                    // Check if abort was triggered during post confirmation
                                                    if self.session.state == RallyState::Aborted {
                                                        return Ok(RallyResult::Aborted {
                                                            iteration,
                                                            reason: e.to_string(),
                                                        });
                                                    }
                                                    warn!("Failed to post follow-up fix comment to PR: {}", e);
                                                }

                                                self.send_event(RallyEvent::FixCompleted(
                                                    output.clone(),
                                                ))
                                                .await;
                                                self.last_fix = Some(output);
                                            }
                                            Err(e) => {
                                                // Clear last_fix to prevent referencing stale value
                                                self.last_fix = None;
                                                self.send_event(RallyEvent::Log(format!(
                                                    "Error continuing after permission denial: {}. Proceeding to re-review.",
                                                    e
                                                )))
                                                .await;
                                            }
                                        }

                                        // Notify TUI of state change
                                        self.session.update_state(RallyState::RevieweeFix);
                                        self.send_event(RallyEvent::StateChanged(
                                            RallyState::RevieweeFix,
                                        ))
                                        .await;
                                        let _ = write_session(&self.session);
                                        // Continue loop
                                    }
                                    break;
                                }
                                Some(OrchestratorCommand::Abort) | None => {
                                    let reason = format!("Permission aborted: {}", perm.action);
                                    self.session.update_state(RallyState::Aborted);
                                    let _ = write_session(&self.session);
                                    self.send_event(RallyEvent::Log(reason.clone())).await;
                                    self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                                        .await;
                                    return Ok(RallyResult::Aborted { iteration, reason });
                                }
                                _ => {
                                    // Stale/invalid command for this state (e.g. PostConfirmResponse) - ignore and re-wait
                                    warn!("Received invalid command during WaitingForPermission, ignoring");
                                    self.send_event(RallyEvent::Log(
                                        "Received invalid command, still waiting for permission...".to_string(),
                                    ))
                                    .await;
                                    continue;
                                }
                            }
                        }
                    }
                }
                RevieweeStatus::Error => {
                    self.session.update_state(RallyState::Error);
                    let _ = write_session(&self.session);

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

        // Max iterations reached is a terminal state (not an error)
        self.session.update_state(RallyState::Completed);
        if let Err(e) = write_session(&self.session) {
            warn!("Failed to write session: {}", e);
        }

        self.send_event(RallyEvent::Log(format!(
            "Max iterations ({}) reached",
            self.config.max_iterations
        )))
        .await;
        self.send_event(RallyEvent::StateChanged(RallyState::Completed))
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
        let _ = write_session(&self.session);

        Ok(())
    }

    /// Handle permission granted from user
    async fn handle_permission_granted(&mut self, action: &str) -> Result<()> {
        // In local mode, validate that the action doesn't contain blocked git operations.
        // Uses strict token-based parsing to prevent bypasses like
        // `git status && git push` passing a substring-based check.
        if self.context.as_ref().is_some_and(|c| c.local_mode) {
            if let Some(reason) = check_blocked_git_operation(action) {
                let msg = format!(
                    "Permission blocked in local mode: {}. Action: {}",
                    reason, action
                );
                warn!("{}", msg);
                self.send_event(RallyEvent::Log(msg.clone())).await;
                return Err(anyhow!(msg));
            }
        }

        self.send_event(RallyEvent::Log(format!(
            "User granted permission for: {}",
            action
        )))
        .await;

        // Add the granted action to reviewee's allowed tools
        // This allows the reviewee to execute the action without being blocked
        self.reviewee_adapter.add_reviewee_allowed_tool(action);

        let prompt = build_permission_granted_prompt(action);
        self.reviewee_adapter.continue_reviewee(&prompt).await?;

        self.session.update_state(RallyState::RevieweeFix);
        self.send_event(RallyEvent::StateChanged(RallyState::RevieweeFix))
            .await;
        let _ = write_session(&self.session);

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

    /// Wrapper that optionally asks for user confirmation before posting review.
    /// - local_mode: skip posting entirely
    /// - auto_post: post directly without confirmation
    /// - otherwise: send confirmation event and wait for user response
    async fn maybe_post_review_to_pr(&mut self, review: &ReviewerOutput) -> Result<()> {
        // local_mode is handled inside post_review_to_pr
        if self.context.as_ref().is_some_and(|c| c.local_mode) {
            return self.post_review_to_pr(review).await;
        }

        if self.config.auto_post {
            return self.post_review_to_pr(review).await;
        }

        // Send confirmation event with lightweight DTO
        let info = ReviewPostInfo {
            action: format!("{:?}", review.action),
            summary: review.summary.clone(),
            comment_count: review.comments.len(),
        };

        self.session
            .update_state(RallyState::WaitingForPostConfirmation);
        let _ = write_session(&self.session);
        self.send_event(RallyEvent::ReviewPostConfirmNeeded(info))
            .await;
        self.send_event(RallyEvent::StateChanged(
            RallyState::WaitingForPostConfirmation,
        ))
        .await;

        // Wait for user response (loop to ignore invalid commands)
        loop {
            match self.wait_for_command().await {
                Some(OrchestratorCommand::PostConfirmResponse(true)) => {
                    self.send_event(RallyEvent::Log(
                        "User approved review posting".to_string(),
                    ))
                    .await;
                    return self.post_review_to_pr(review).await;
                }
                Some(OrchestratorCommand::PostConfirmResponse(false)) => {
                    self.send_event(RallyEvent::Log(
                        "User skipped review posting".to_string(),
                    ))
                    .await;
                    return Ok(());
                }
                Some(OrchestratorCommand::Abort) | None => {
                    self.session.update_state(RallyState::Aborted);
                    let _ = write_session(&self.session);
                    self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                        .await;
                    return Err(anyhow!("Review posting aborted by user"));
                }
                _ => {
                    // Invalid command for this state - warn and re-wait
                    warn!("Received invalid command during WaitingForPostConfirmation, ignoring");
                    continue;
                }
            }
        }
    }

    /// Wrapper that optionally asks for user confirmation before posting fix comment.
    async fn maybe_post_fix_comment(&mut self, fix: &RevieweeOutput) -> Result<()> {
        // local_mode is handled inside post_fix_comment
        if self.context.as_ref().is_some_and(|c| c.local_mode) {
            return self.post_fix_comment(fix).await;
        }

        if self.config.auto_post {
            return self.post_fix_comment(fix).await;
        }

        // Send confirmation event with lightweight DTO
        let info = FixPostInfo {
            summary: fix.summary.clone(),
            files_modified: fix.files_modified.clone(),
        };

        self.session
            .update_state(RallyState::WaitingForPostConfirmation);
        let _ = write_session(&self.session);
        self.send_event(RallyEvent::FixPostConfirmNeeded(info))
            .await;
        self.send_event(RallyEvent::StateChanged(
            RallyState::WaitingForPostConfirmation,
        ))
        .await;

        // Wait for user response (loop to ignore invalid commands)
        loop {
            match self.wait_for_command().await {
                Some(OrchestratorCommand::PostConfirmResponse(true)) => {
                    self.send_event(RallyEvent::Log(
                        "User approved fix comment posting".to_string(),
                    ))
                    .await;
                    return self.post_fix_comment(fix).await;
                }
                Some(OrchestratorCommand::PostConfirmResponse(false)) => {
                    self.send_event(RallyEvent::Log(
                        "User skipped fix comment posting".to_string(),
                    ))
                    .await;
                    return Ok(());
                }
                Some(OrchestratorCommand::Abort) | None => {
                    self.session.update_state(RallyState::Aborted);
                    let _ = write_session(&self.session);
                    self.send_event(RallyEvent::StateChanged(RallyState::Aborted))
                        .await;
                    return Err(anyhow!("Fix comment posting aborted by user"));
                }
                _ => {
                    // Invalid command for this state - warn and re-wait
                    warn!("Received invalid command during WaitingForPostConfirmation, ignoring");
                    continue;
                }
            }
        }
    }

    /// Post review to PR (summary comment + inline comments)
    async fn post_review_to_pr(&self, review: &ReviewerOutput) -> Result<()> {
        if self.context.as_ref().is_some_and(|c| c.local_mode) {
            self.send_event(RallyEvent::Log(
                "Local mode: skipping review posting to PR".to_string(),
            ))
            .await;
            return Ok(());
        }

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
            // Convert line number to patch position
            let patch = context
                .file_patches
                .iter()
                .find(|(name, _)| name == &comment.path)
                .map(|(_, p)| p.as_str());

            let Some(patch) = patch else {
                warn!("No patch found for {}, skipping comment", comment.path);
                continue;
            };

            let Some(position) = crate::diff::line_number_to_position(patch, comment.line) else {
                warn!(
                    "Could not convert line {} to position for {}, skipping comment",
                    comment.line, comment.path
                );
                continue;
            };

            // Add prefix to inline comment
            let body_with_prefix = format!("[AI Rally - Reviewer]\n\n{}", comment.body);
            if let Err(e) = github::create_review_comment(
                &self.repo,
                self.pr_number,
                &context.head_sha,
                &comment.path,
                position,
                &body_with_prefix,
            )
            .await
            {
                warn!(
                    "Failed to post inline comment on {}:{} (position {}): {}",
                    comment.path, comment.line, position, e
                );
            }
            // Rate limit mitigation: small delay between API calls
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }

    /// Post fix summary comment to PR
    async fn post_fix_comment(&self, fix: &RevieweeOutput) -> Result<()> {
        if self.context.as_ref().is_some_and(|c| c.local_mode) {
            self.send_event(RallyEvent::Log(
                "Local mode: skipping fix comment posting".to_string(),
            ))
            .await;
            return Ok(());
        }

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
        if self.context.as_ref().is_some_and(|c| c.local_mode) {
            return Vec::new();
        }

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
        if self.context.as_ref().is_some_and(|c| c.local_mode) {
            return Ok(());
        }

        let pr = github::fetch_pr(&self.repo, self.pr_number).await?;
        if let Some(ref mut ctx) = self.context {
            ctx.head_sha = pr.head.sha.clone();
        }
        Ok(())
    }

    /// Fetch current diff, preferring local git diff over GitHub API.
    ///
    /// This allows the reviewer to see uncommitted/unpushed changes made by the reviewee.
    /// Falls back to GitHub API if local git diff fails or returns empty.
    async fn fetch_current_diff(&self) -> Result<String> {
        // ローカルモードでは git fetch をスキップし、直接 diff を取得
        if let Some(ref ctx) = self.context {
            if ctx.local_mode {
                return self.fetch_local_working_diff(ctx).await;
            }
        }

        // Timeout for git operations (30 seconds)
        const GIT_TIMEOUT_SECS: u64 = 30;

        // Try local git diff first if we have working_dir and base_branch
        if let Some(ref ctx) = self.context {
            if let Some(ref working_dir) = ctx.working_dir {
                let base_branch = &ctx.base_branch;

                // Fetch latest base branch reference to ensure accurate diff
                // Use timeout to prevent hanging on slow remotes or credential prompts
                let fetch_future = tokio::process::Command::new("git")
                    .args(["fetch", "origin", base_branch])
                    .current_dir(working_dir)
                    .output();

                match timeout(Duration::from_secs(GIT_TIMEOUT_SECS), fetch_future).await {
                    Ok(Ok(output)) if output.status.success() => {
                        // Fetch succeeded
                    }
                    Ok(Ok(_)) => {
                        warn!("git fetch failed, continuing with potentially stale ref");
                    }
                    Ok(Err(e)) => {
                        warn!(
                            "git fetch command failed: {}, continuing with potentially stale ref",
                            e
                        );
                    }
                    Err(_) => {
                        warn!(
                            "git fetch timed out after {} seconds, continuing with potentially stale ref",
                            GIT_TIMEOUT_SECS
                        );
                    }
                }

                // Try git diff against origin/base_branch using merge-base (three-dot) comparison
                // This matches GitHub PR diff semantics and avoids including unrelated base-branch changes
                // Wrap in timeout to prevent hanging on network issues or auth prompts
                let git_diff_future = tokio::process::Command::new("git")
                    .args(["diff", &format!("origin/{}...HEAD", base_branch)])
                    .current_dir(working_dir)
                    .output();

                match timeout(Duration::from_secs(GIT_TIMEOUT_SECS), git_diff_future).await {
                    Ok(Ok(output)) if output.status.success() => {
                        let diff = String::from_utf8_lossy(&output.stdout).to_string();
                        if !diff.trim().is_empty() {
                            self.send_event(RallyEvent::Log(
                                "Using local git diff for re-review".to_string(),
                            ))
                            .await;
                            return Ok(diff);
                        }
                    }
                    Ok(Ok(_)) => {
                        // git diff failed, fall through to GitHub API
                    }
                    Ok(Err(e)) => {
                        warn!("git diff command failed: {}", e);
                    }
                    Err(_) => {
                        warn!(
                            "git diff timed out after {} seconds, falling back to GitHub API",
                            GIT_TIMEOUT_SECS
                        );
                    }
                }

                self.send_event(RallyEvent::Log(
                    "Local git diff empty or failed, falling back to GitHub API".to_string(),
                ))
                .await;
            }
        }

        // Fallback to GitHub API
        github::fetch_pr_diff(&self.repo, self.pr_number).await
    }

    /// ローカルモード専用の diff 取得
    ///
    /// `git diff HEAD` を最優先し、working tree + staged の最新変更を取得。
    /// 空の場合は `origin/{base}...HEAD` でコミット済み差分を試行。
    /// どちらも空の場合は空文字列を返す（stale な初期 diff にフォールバックしない）。
    async fn fetch_local_working_diff(&self, ctx: &super::adapter::Context) -> Result<String> {
        const GIT_TIMEOUT_SECS: u64 = 30;

        let working_dir = ctx.working_dir.as_deref().unwrap_or(".");
        let base_branch = &ctx.base_branch;

        // 1. git diff HEAD（working tree + staged の最新変更を優先）
        let git_diff_future = tokio::process::Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(working_dir)
            .output();

        match timeout(Duration::from_secs(GIT_TIMEOUT_SECS), git_diff_future).await {
            Ok(Ok(output)) if output.status.success() => {
                let diff = String::from_utf8_lossy(&output.stdout).to_string();
                if !diff.trim().is_empty() {
                    self.send_event(RallyEvent::Log(
                        "Using local git diff HEAD for re-review".to_string(),
                    ))
                    .await;
                    return Ok(diff);
                }
            }
            _ => {}
        }

        // 2. Fallback: origin/{base}...HEAD（コミット済み差分）
        let origin_ref = format!("origin/{}...HEAD", base_branch);
        let git_diff_future = tokio::process::Command::new("git")
            .args(["diff", &origin_ref])
            .current_dir(working_dir)
            .output();

        if let Ok(Ok(output)) =
            timeout(Duration::from_secs(GIT_TIMEOUT_SECS), git_diff_future).await
        {
            if output.status.success() {
                let diff = String::from_utf8_lossy(&output.stdout).to_string();
                if !diff.trim().is_empty() {
                    self.send_event(RallyEvent::Log(
                        "Using local git diff (origin base) for re-review".to_string(),
                    ))
                    .await;
                    return Ok(diff);
                }
            }
        }

        // 両方空の場合は空文字列を返す（stale な ctx.diff にフォールバックしない）
        self.send_event(RallyEvent::Log(
            "Local diff is empty (no changes detected)".to_string(),
        ))
        .await;
        Ok(String::new())
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

/// Extract the shell command from a `Bash(command:*)` tool pattern.
///
/// Returns `Some(command)` if the action matches the pattern, `None` otherwise.
fn extract_bash_command(action: &str) -> Option<&str> {
    let rest = action.trim().strip_prefix("Bash(")?;
    // Handle both Bash(cmd:*) and Bash(cmd) formats
    let inner = rest.strip_suffix(')')?;
    Some(inner.strip_suffix(":*").unwrap_or(inner))
}

/// Split a shell command string by command separators (`&&`, `||`, `;`, `|`, `&`, newline).
///
/// Handles two-character separators (`&&`, `||`) before single-character ones
/// (`&`, `|`) to avoid incorrect splitting. Newlines (`\n`) are also treated as
/// command separators to prevent bypass via multi-line commands.
fn split_shell_commands(command: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut start = 0;
    let bytes = command.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Check for two-character separators (&& and ||) first
        let is_double =
            i + 1 < len && ((bytes[i] == b'&' && bytes[i + 1] == b'&') || (bytes[i] == b'|' && bytes[i + 1] == b'|'));
        if is_double {
            results.push(&command[start..i]);
            i += 2;
            start = i;
        } else if bytes[i] == b';' || bytes[i] == b'|' || bytes[i] == b'&' || bytes[i] == b'\n' {
            results.push(&command[start..i]);
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }

    if start <= len {
        results.push(&command[start..]);
    }

    results
}

/// Shell wrapper commands that can prefix the actual binary.
/// e.g., `env git push`, `command git push`, `sudo git push`
const SHELL_WRAPPERS: &[&str] = &[
    "env", "command", "builtin", "exec", "nohup", "nice", "sudo", "xargs",
];

/// Shell interpreters that can execute arbitrary command strings via `-c`.
/// e.g., `sh -c 'git push'`, `bash -lc "git push"`
const SHELL_INTERPRETERS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "fish"];

/// Maximum recursion depth for nested shell interpreter detection.
const MAX_SHELL_NESTING_DEPTH: usize = 3;

/// Check if a token looks like an environment variable assignment (VAR=value).
fn is_env_var_assignment(token: &str) -> bool {
    if let Some(eq_pos) = token.find('=') {
        // Must have at least one char before '=' and the prefix must be a valid env var name
        eq_pos > 0
            && token[..eq_pos]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
    } else {
        false
    }
}

/// Check if a token refers to the `git` binary, handling absolute/relative paths.
/// e.g., "git", "/usr/bin/git", "./git", "../bin/git"
fn is_git_binary(token: &str) -> bool {
    // Extract basename: everything after the last '/'
    let basename = match token.rfind('/') {
        Some(pos) => &token[pos + 1..],
        None => token,
    };
    basename == "git"
}

/// Check if a token refers to a shell interpreter, handling absolute paths.
/// e.g., "sh", "bash", "/usr/bin/bash", "/bin/sh"
fn is_shell_interpreter(token: &str) -> bool {
    let basename = match token.rfind('/') {
        Some(pos) => &token[pos + 1..],
        None => token,
    };
    SHELL_INTERPRETERS.contains(&basename)
}

/// Extract the command string from a shell interpreter invocation with `-c`.
///
/// Handles:
/// - `sh -c 'git push'` → "git push"
/// - `bash -lc "git status && git push"` → "git status && git push"
/// - `env sh -c "git push"` → "git push"
/// - `/usr/bin/bash -c git push` → "git" (only the first argument; `push` becomes `$0`)
///
/// In POSIX shell, `sh -c cmd_string [command_name [argument...]]` — only the first
/// argument after `-c` is the command string. Subsequent arguments are positional
/// parameters (`$0`, `$1`, ...), not part of the executed command.
///
/// Returns `None` if the command is not a shell interpreter invocation with `-c`.
fn extract_shell_interpreter_command(command: &str) -> Option<String> {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let mut i = 0;

    // Skip leading env var assignments (e.g., VAR=val sh -c ...)
    while i < tokens.len() && is_env_var_assignment(tokens[i]) {
        i += 1;
    }

    // Skip shell wrappers (env, command, sudo, etc.)
    while i < tokens.len() {
        let basename = match tokens[i].rfind('/') {
            Some(pos) => &tokens[i][pos + 1..],
            None => tokens[i],
        };
        if SHELL_WRAPPERS.contains(&basename) {
            i += 1;
            // Skip wrapper flags
            while i < tokens.len() && tokens[i].starts_with('-') {
                i += 1;
                if i < tokens.len()
                    && !is_shell_interpreter(tokens[i])
                    && !is_git_binary(tokens[i])
                    && !tokens[i].starts_with('-')
                {
                    i += 1;
                }
            }
            // Skip env var assignments after wrapper
            while i < tokens.len() && is_env_var_assignment(tokens[i]) {
                i += 1;
            }
        } else {
            break;
        }
    }

    if i >= tokens.len() {
        return None;
    }

    // Check if it's a shell interpreter
    if !is_shell_interpreter(tokens[i]) {
        return None;
    }
    i += 1;

    // Look for -c flag (standalone or combined like -lc, -ic)
    let mut found_c = false;
    while i < tokens.len() && tokens[i].starts_with('-') {
        let flag = tokens[i];
        // Standalone -c, or combined short flags ending with 'c' (e.g., -lc, -ic)
        // The 'c' must be last since it takes the next argument as the command string
        if flag == "-c"
            || (flag.len() >= 2
                && flag.starts_with('-')
                && !flag.starts_with("--")
                && flag.ends_with('c'))
        {
            found_c = true;
            i += 1;
            break;
        }
        i += 1;
    }

    if !found_c || i >= tokens.len() {
        return None;
    }

    // Extract only the first shell argument after -c (the command string).
    // In POSIX sh, `sh -c cmd_string [arg0 ...]` — only cmd_string is executed;
    // subsequent arguments become positional parameters ($0, $1, ...).
    let first_token = tokens[i];
    let first_char = first_token.chars().next()?;

    if first_char == '\'' || first_char == '"' {
        // Quoted argument: collect tokens until the matching closing quote
        if first_token.len() > 1 && first_token.ends_with(first_char) {
            // Entire quoted argument in one token, e.g., 'cmd' or "cmd"
            Some(first_token[1..first_token.len() - 1].to_string())
        } else {
            // Quote spans multiple whitespace-separated tokens
            let mut end = i + 1;
            while end < tokens.len() && !tokens[end].ends_with(first_char) {
                end += 1;
            }
            if end < tokens.len() {
                // Found closing quote — join and strip outer quotes
                let cmd_str = tokens[i..=end].join(" ");
                Some(cmd_str[1..cmd_str.len() - 1].to_string())
            } else {
                // No closing quote found — best effort: strip leading quote
                let cmd_str = tokens[i..].join(" ");
                Some(cmd_str[1..].to_string())
            }
        }
    } else {
        // Unquoted: only the first token is the command string
        Some(first_token.to_string())
    }
}

/// Find the git binary and its subcommand index within a token list,
/// skipping environment variable assignments (VAR=value) and shell wrappers
/// (env, command, sudo, etc.).
///
/// Returns `Some((git_index, subcommand_index))` if git is found, `None` otherwise.
fn find_git_in_tokens<'a>(tokens: &[&'a str]) -> Option<(usize, Option<usize>)> {
    let mut i = 0;

    // Skip leading env var assignments (e.g., GIT_TRACE=1 VAR=val git push)
    while i < tokens.len() && is_env_var_assignment(tokens[i]) {
        i += 1;
    }

    // Skip shell wrapper commands (e.g., env, command, sudo)
    // Wrappers can also have their own flags (e.g., `env -i git push`)
    while i < tokens.len() {
        let basename = match tokens[i].rfind('/') {
            Some(pos) => &tokens[i][pos + 1..],
            None => tokens[i],
        };
        if SHELL_WRAPPERS.contains(&basename) {
            i += 1;
            // Skip wrapper flags (e.g., `env -i`, `nice -n 5`, `sudo -u user`)
            while i < tokens.len() && tokens[i].starts_with('-') {
                i += 1;
                // Also skip the argument to the flag if it's not combined (e.g., `-n 5`)
                // We conservatively skip one more token for flags that take args,
                // but only if the next token doesn't look like git
                if i < tokens.len() && !is_git_binary(tokens[i]) && !tokens[i].starts_with('-') {
                    i += 1;
                }
            }
            // After wrapper flags, skip env var assignments again
            // (e.g., `env VAR=val git push`)
            while i < tokens.len() && is_env_var_assignment(tokens[i]) {
                i += 1;
            }
        } else {
            break;
        }
    }

    if i >= tokens.len() {
        return None;
    }

    if is_git_binary(tokens[i]) {
        let git_idx = i;
        let sub_idx = if i + 1 < tokens.len() {
            Some(i + 1)
        } else {
            None
        };
        Some((git_idx, sub_idx))
    } else {
        None
    }
}

/// Validate whether a tool/action string contains blocked git operations.
///
/// Uses strict token-based parsing instead of substring matching to prevent
/// bypasses like `git status && git push` passing a `contains("git status")` check.
///
/// Also handles wrapper commands (`env`, `command`, `sudo`, etc.), environment
/// variable prefixes (`VAR=val git push`), absolute paths (`/usr/bin/git push`),
/// and nested shell interpreters (`sh -c 'git push'`, `bash -lc "git push"`)
/// that could be used to bypass a simple `tokens[0] == "git"` check.
///
/// Returns `Some(reason)` if the action is blocked, `None` if allowed.
fn check_blocked_git_operation(action: &str) -> Option<String> {
    // For non-Bash tools (Read, Edit, Write, Glob, Grep, etc.), always allow
    let command = match extract_bash_command(action) {
        Some(cmd) => cmd,
        None => {
            // Not a Bash() pattern — check if it looks like a raw git command
            let trimmed = action.trim();
            if trimmed.starts_with("git ") || trimmed == "git" {
                trimmed
            } else {
                return None;
            }
        }
    };

    if command.is_empty() {
        return None;
    }

    check_command_for_blocked_git(command, 0)
}

/// Recursive inner function that checks a command string for blocked git operations.
/// The `depth` parameter prevents infinite recursion from deeply nested shell invocations.
fn check_command_for_blocked_git(command: &str, depth: usize) -> Option<String> {
    if depth > MAX_SHELL_NESTING_DEPTH {
        return Some("Shell command nesting too deep — blocked for safety".to_string());
    }

    // Split by shell command separators to detect chained commands
    let individual_commands = split_shell_commands(command);

    for cmd in &individual_commands {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        // Find the git binary in the token list, skipping env vars and wrappers
        if let Some((_, sub_idx)) = find_git_in_tokens(&tokens) {
            match sub_idx {
                None => {
                    return Some(
                        "Bare 'git' command without subcommand is not allowed".to_string(),
                    );
                }
                Some(si) => {
                    let subcommand = tokens[si];

                    // Reject flags before subcommand (e.g., git -C /path push)
                    // as they can be used to obfuscate the actual operation
                    if subcommand.starts_with('-') {
                        return Some(format!(
                            "Git command with flags before subcommand is not allowed: '{}'",
                            trimmed
                        ));
                    }

                    if !ALLOWED_GIT_SUBCOMMANDS.contains(&subcommand) {
                        return Some(format!(
                            "Git subcommand '{}' is not in the allowed list ({:?})",
                            subcommand, ALLOWED_GIT_SUBCOMMANDS
                        ));
                    }
                }
            }
        }

        // Check for shell interpreter with -c (e.g., sh -c 'git push', bash -lc "git push")
        // This catches nested execution that bypasses direct git binary detection.
        if let Some(inner_cmd) = extract_shell_interpreter_command(trimmed) {
            if let Some(reason) = check_command_for_blocked_git(&inner_cmd, depth + 1) {
                return Some(reason);
            }
        }
    }

    None
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

        // Test SkipClarification
        let cmd = OrchestratorCommand::SkipClarification;
        assert!(matches!(cmd, OrchestratorCommand::SkipClarification));

        // Test PostConfirmResponse approved
        let cmd = OrchestratorCommand::PostConfirmResponse(true);
        match cmd {
            OrchestratorCommand::PostConfirmResponse(approved) => {
                assert!(approved);
            }
            _ => panic!("Expected PostConfirmResponse"),
        }

        // Test PostConfirmResponse skipped
        let cmd = OrchestratorCommand::PostConfirmResponse(false);
        match cmd {
            OrchestratorCommand::PostConfirmResponse(approved) => {
                assert!(!approved);
            }
            _ => panic!("Expected PostConfirmResponse"),
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
    async fn test_command_channel_skip_clarification() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        tx.send(OrchestratorCommand::SkipClarification)
            .await
            .unwrap();

        let cmd = rx.recv().await.unwrap();
        assert!(matches!(cmd, OrchestratorCommand::SkipClarification));
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

    #[tokio::test]
    async fn test_command_channel_post_confirm_approved() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        tx.send(OrchestratorCommand::PostConfirmResponse(true))
            .await
            .unwrap();

        let cmd = rx.recv().await.unwrap();
        match cmd {
            OrchestratorCommand::PostConfirmResponse(approved) => {
                assert!(approved, "Post should be approved");
            }
            _ => panic!("Expected PostConfirmResponse"),
        }
    }

    #[tokio::test]
    async fn test_command_channel_post_confirm_skipped() {
        let (tx, mut rx) = mpsc::channel::<OrchestratorCommand>(1);

        tx.send(OrchestratorCommand::PostConfirmResponse(false))
            .await
            .unwrap();

        let cmd = rx.recv().await.unwrap();
        match cmd {
            OrchestratorCommand::PostConfirmResponse(approved) => {
                assert!(!approved, "Post should be skipped");
            }
            _ => panic!("Expected PostConfirmResponse"),
        }
    }

    #[test]
    fn test_rally_state_is_active() {
        assert!(RallyState::Initializing.is_active());
        assert!(RallyState::ReviewerReviewing.is_active());
        assert!(RallyState::RevieweeFix.is_active());
        assert!(RallyState::WaitingForClarification.is_active());
        assert!(RallyState::WaitingForPermission.is_active());
        assert!(RallyState::WaitingForPostConfirmation.is_active());
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
        assert!(!RallyState::WaitingForPostConfirmation.is_finished());
        assert!(RallyState::Completed.is_finished());
        assert!(RallyState::Aborted.is_finished());
        assert!(RallyState::Error.is_finished());
    }

    #[test]
    fn test_review_post_info() {
        let info = ReviewPostInfo {
            action: "Approve".to_string(),
            summary: "Looks good".to_string(),
            comment_count: 3,
        };
        assert_eq!(info.action, "Approve");
        assert_eq!(info.summary, "Looks good");
        assert_eq!(info.comment_count, 3);
    }

    #[test]
    fn test_fix_post_info() {
        let info = FixPostInfo {
            summary: "Fixed issues".to_string(),
            files_modified: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
        };
        assert_eq!(info.summary, "Fixed issues");
        assert_eq!(info.files_modified.len(), 2);
    }

    #[test]
    fn test_extract_bash_command() {
        // Standard Bash(cmd:*) format
        assert_eq!(extract_bash_command("Bash(git push:*)"), Some("git push"));
        assert_eq!(
            extract_bash_command("Bash(git status:*)"),
            Some("git status")
        );

        // Without wildcard suffix
        assert_eq!(
            extract_bash_command("Bash(git push)"),
            Some("git push")
        );

        // Not a Bash pattern
        assert_eq!(extract_bash_command("Read"), None);
        assert_eq!(extract_bash_command("Edit"), None);
        assert_eq!(extract_bash_command("git push"), None);

        // Complex commands
        assert_eq!(
            extract_bash_command("Bash(git status && git push:*)"),
            Some("git status && git push")
        );
    }

    #[test]
    fn test_split_shell_commands() {
        // Single command
        assert_eq!(split_shell_commands("git status"), vec!["git status"]);

        // && separator
        let result = split_shell_commands("git status && git push");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].trim(), "git status");
        assert_eq!(result[1].trim(), "git push");

        // || separator
        let result = split_shell_commands("git status || git push");
        assert_eq!(result.len(), 2);

        // ; separator
        let result = split_shell_commands("git status; git push");
        assert_eq!(result.len(), 2);

        // | pipe
        let result = split_shell_commands("echo test | git push");
        assert_eq!(result.len(), 2);

        // Multiple separators
        let result = split_shell_commands("git status && git diff; git push");
        assert_eq!(result.len(), 3);

        // & (background) separator
        let result = split_shell_commands("git status & git push");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].trim(), "git status");
        assert_eq!(result[1].trim(), "git push");

        // Newline separator
        let result = split_shell_commands("git status\ngit push");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].trim(), "git status");
        assert_eq!(result[1].trim(), "git push");

        // Multiple newlines
        let result = split_shell_commands("git status\ngit diff\ngit push");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_check_blocked_git_operation_allows_safe_commands() {
        // Allowed git subcommands
        assert!(check_blocked_git_operation("Bash(git status:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git diff:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git add:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git commit:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git log:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git show:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git branch:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git switch:*)").is_none());
        assert!(check_blocked_git_operation("Bash(git stash:*)").is_none());

        // Non-Bash tools are always allowed
        assert!(check_blocked_git_operation("Read").is_none());
        assert!(check_blocked_git_operation("Edit").is_none());
        assert!(check_blocked_git_operation("Write").is_none());
        assert!(check_blocked_git_operation("Glob").is_none());
        assert!(check_blocked_git_operation("Grep").is_none());
        assert!(check_blocked_git_operation("Skill").is_none());

        // Non-git Bash commands are allowed
        assert!(check_blocked_git_operation("Bash(cargo test:*)").is_none());
        assert!(check_blocked_git_operation("Bash(npm run build:*)").is_none());
    }

    #[test]
    fn test_check_blocked_git_operation_blocks_write_operations() {
        // git push
        assert!(check_blocked_git_operation("Bash(git push:*)").is_some());
        assert!(check_blocked_git_operation("Bash(git push origin main:*)").is_some());

        // git reset
        assert!(check_blocked_git_operation("Bash(git reset --hard:*)").is_some());

        // git checkout (can discard changes)
        assert!(check_blocked_git_operation("Bash(git checkout:*)").is_some());

        // git restore (can discard changes)
        assert!(check_blocked_git_operation("Bash(git restore:*)").is_some());

        // git clean
        assert!(check_blocked_git_operation("Bash(git clean:*)").is_some());

        // git rebase
        assert!(check_blocked_git_operation("Bash(git rebase:*)").is_some());

        // git merge
        assert!(check_blocked_git_operation("Bash(git merge:*)").is_some());
    }

    #[test]
    fn test_check_blocked_git_operation_blocks_mixed_commands() {
        // Mixed read + write via &&
        assert!(
            check_blocked_git_operation("Bash(git status && git push:*)").is_some(),
            "Should block git push hidden after git status via &&"
        );

        // Mixed read + write via ;
        assert!(
            check_blocked_git_operation("Bash(git status; git push -f origin main:*)").is_some(),
            "Should block git push hidden after git status via ;"
        );

        // Mixed read + write via ||
        assert!(
            check_blocked_git_operation("Bash(git status || git reset --hard:*)").is_some(),
            "Should block git reset hidden after git status via ||"
        );

        // Mixed read + write via pipe
        assert!(
            check_blocked_git_operation("Bash(echo test | git push:*)").is_some(),
            "Should block git push via pipe"
        );

        // Mixed read + write via & (background)
        assert!(
            check_blocked_git_operation("Bash(git status & git push:*)").is_some(),
            "Should block git push hidden after git status via &"
        );

        // Mixed read + write via newline
        assert!(
            check_blocked_git_operation("Bash(git status\ngit push:*)").is_some(),
            "Should block git push hidden after git status via newline"
        );

        // Multiple newlines with write at end
        assert!(
            check_blocked_git_operation("Bash(git status\ngit diff\ngit push:*)").is_some(),
            "Should block git push hidden in multi-line command"
        );

        // Multiple chained safe commands should be allowed
        assert!(
            check_blocked_git_operation("Bash(git status && git diff:*)").is_none(),
            "Should allow chained safe git commands"
        );

        // Safe commands via & should be allowed
        assert!(
            check_blocked_git_operation("Bash(git status & git diff:*)").is_none(),
            "Should allow chained safe git commands via &"
        );

        // Safe commands via newline should be allowed
        assert!(
            check_blocked_git_operation("Bash(git status\ngit diff:*)").is_none(),
            "Should allow chained safe git commands via newline"
        );
    }

    #[test]
    fn test_check_blocked_git_operation_blocks_flag_obfuscation() {
        // git -C /path push (flag before subcommand to obfuscate)
        assert!(
            check_blocked_git_operation("Bash(git -C /tmp push:*)").is_some(),
            "Should block git commands with flags before subcommand"
        );
    }

    #[test]
    fn test_check_blocked_git_operation_raw_commands() {
        // Raw git commands (not in Bash() format)
        assert!(check_blocked_git_operation("git push").is_some());
        assert!(check_blocked_git_operation("git status").is_none());
        assert!(check_blocked_git_operation("git reset --hard").is_some());
    }

    #[test]
    fn test_check_blocked_bare_git() {
        // Bare 'git' without subcommand
        assert!(check_blocked_git_operation("Bash(git:*)").is_some());
        assert!(check_blocked_git_operation("git").is_some());
    }

    #[test]
    fn test_check_blocked_non_git_with_git_substring() {
        // Words containing "git" that aren't git commands should not be blocked
        // (e.g., "widget", "digit", "legit")
        assert!(check_blocked_git_operation("Bash(widget build:*)").is_none());
        assert!(check_blocked_git_operation("Bash(cargo test --features digit:*)").is_none());
    }

    #[test]
    fn test_check_blocked_git_via_env_wrapper() {
        // env git push — bypasses tokens[0] == "git" check
        assert!(
            check_blocked_git_operation("Bash(env git push:*)").is_some(),
            "Should block 'env git push'"
        );
        // env with flags
        assert!(
            check_blocked_git_operation("Bash(env -i git push:*)").is_some(),
            "Should block 'env -i git push'"
        );
        // env with safe git command should be allowed
        assert!(
            check_blocked_git_operation("Bash(env git status:*)").is_none(),
            "Should allow 'env git status'"
        );
    }

    #[test]
    fn test_check_blocked_git_via_command_wrapper() {
        // command git push
        assert!(
            check_blocked_git_operation("Bash(command git push:*)").is_some(),
            "Should block 'command git push'"
        );
        // command with safe subcommand
        assert!(
            check_blocked_git_operation("Bash(command git diff:*)").is_none(),
            "Should allow 'command git diff'"
        );
    }

    #[test]
    fn test_check_blocked_git_via_env_var_prefix() {
        // GIT_TRACE=1 git push — env var assignment before git
        assert!(
            check_blocked_git_operation("Bash(GIT_TRACE=1 git push:*)").is_some(),
            "Should block 'GIT_TRACE=1 git push'"
        );
        // Multiple env vars
        assert!(
            check_blocked_git_operation("Bash(GIT_TRACE=1 GIT_CURL_VERBOSE=1 git push:*)").is_some(),
            "Should block git push with multiple env var prefixes"
        );
        // Env var with safe command
        assert!(
            check_blocked_git_operation("Bash(GIT_TRACE=1 git status:*)").is_none(),
            "Should allow 'GIT_TRACE=1 git status'"
        );
    }

    #[test]
    fn test_check_blocked_git_via_absolute_path() {
        // /usr/bin/git push — absolute path to git
        assert!(
            check_blocked_git_operation("Bash(/usr/bin/git push:*)").is_some(),
            "Should block '/usr/bin/git push'"
        );
        // /usr/local/bin/git push
        assert!(
            check_blocked_git_operation("Bash(/usr/local/bin/git push:*)").is_some(),
            "Should block '/usr/local/bin/git push'"
        );
        // Absolute path with safe command
        assert!(
            check_blocked_git_operation("Bash(/usr/bin/git status:*)").is_none(),
            "Should allow '/usr/bin/git status'"
        );
    }

    #[test]
    fn test_check_blocked_git_combined_wrappers() {
        // env + env var + absolute path
        assert!(
            check_blocked_git_operation("Bash(env GIT_TRACE=1 /usr/bin/git push:*)").is_some(),
            "Should block 'env GIT_TRACE=1 /usr/bin/git push'"
        );
        // command + env var + git push
        assert!(
            check_blocked_git_operation("Bash(command GIT_TRACE=1 git push:*)").is_some(),
            "Should block 'command GIT_TRACE=1 git push'"
        );
        // Chained: safe wrapper command && wrapper push
        assert!(
            check_blocked_git_operation("Bash(git status && env git push:*)").is_some(),
            "Should block 'env git push' hidden after safe command via &&"
        );
        // Chained: env var safe && absolute path push
        assert!(
            check_blocked_git_operation("Bash(GIT_TRACE=1 git status; /usr/bin/git push:*)").is_some(),
            "Should block '/usr/bin/git push' hidden after safe command via ;"
        );
    }

    #[test]
    fn test_check_blocked_git_via_shell_interpreter_c() {
        // sh -c 'git push'
        assert!(
            check_blocked_git_operation("Bash(sh -c 'git push':*)").is_some(),
            "Should block 'sh -c git push'"
        );
        // bash -c "git push"
        assert!(
            check_blocked_git_operation("Bash(bash -c \"git push\":*)").is_some(),
            "Should block 'bash -c git push'"
        );
        // zsh -c 'git push origin main'
        assert!(
            check_blocked_git_operation("Bash(zsh -c 'git push origin main':*)").is_some(),
            "Should block 'zsh -c git push origin main'"
        );
        // bash with combined flags: -lc
        assert!(
            check_blocked_git_operation("Bash(bash -lc \"git push\":*)").is_some(),
            "Should block 'bash -lc git push'"
        );
        // bash -lc with chained commands hiding git push
        assert!(
            check_blocked_git_operation("Bash(bash -lc \"git status && git push\":*)").is_some(),
            "Should block 'bash -lc \"git status && git push\"'"
        );
        // sh -c without quotes
        assert!(
            check_blocked_git_operation("Bash(sh -c git push:*)").is_some(),
            "Should block 'sh -c git push' (unquoted)"
        );
        // Absolute path to shell interpreter
        assert!(
            check_blocked_git_operation("Bash(/bin/sh -c 'git push':*)").is_some(),
            "Should block '/bin/sh -c git push'"
        );
        assert!(
            check_blocked_git_operation("Bash(/usr/bin/bash -c 'git push':*)").is_some(),
            "Should block '/usr/bin/bash -c git push'"
        );
        // env wrapper + shell interpreter
        assert!(
            check_blocked_git_operation("Bash(env sh -c 'git push':*)").is_some(),
            "Should block 'env sh -c git push'"
        );
        // dash, ksh, fish
        assert!(
            check_blocked_git_operation("Bash(dash -c 'git push':*)").is_some(),
            "Should block 'dash -c git push'"
        );
        assert!(
            check_blocked_git_operation("Bash(ksh -c 'git push':*)").is_some(),
            "Should block 'ksh -c git push'"
        );
        assert!(
            check_blocked_git_operation("Bash(fish -c 'git push':*)").is_some(),
            "Should block 'fish -c git push'"
        );
        // Chained: safe command && shell interpreter git push
        assert!(
            check_blocked_git_operation("Bash(echo hello && sh -c 'git push':*)").is_some(),
            "Should block shell interpreter git push in chained command"
        );
        // Nested: bash -c "sh -c 'git push'"
        assert!(
            check_blocked_git_operation("Bash(bash -c \"sh -c 'git push'\":*)").is_some(),
            "Should block doubly nested shell interpreter git push"
        );
        // Regression: trailing args after -c command string must not bypass detection
        assert!(
            check_blocked_git_operation("Bash(bash -c \"git push\" x:*)").is_some(),
            "Should block 'bash -c \"git push\" x' — trailing arg must not bypass detection"
        );
        assert!(
            check_blocked_git_operation("Bash(sh -c 'git push' --:*)").is_some(),
            "Should block 'sh -c 'git push' --' — trailing '--' must not bypass detection"
        );
    }

    #[test]
    fn test_check_blocked_git_via_shell_interpreter_allows_safe() {
        // sh -c with safe git command
        assert!(
            check_blocked_git_operation("Bash(sh -c 'git status':*)").is_none(),
            "Should allow 'sh -c git status'"
        );
        assert!(
            check_blocked_git_operation("Bash(bash -c 'git diff':*)").is_none(),
            "Should allow 'bash -c git diff'"
        );
        // sh -c with non-git command
        assert!(
            check_blocked_git_operation("Bash(sh -c 'echo hello':*)").is_none(),
            "Should allow 'sh -c echo hello'"
        );
        // bash without -c (just running a script)
        assert!(
            check_blocked_git_operation("Bash(bash script.sh:*)").is_none(),
            "Should allow 'bash script.sh' (no -c flag)"
        );
        // sh with other flags but no -c
        assert!(
            check_blocked_git_operation("Bash(sh -x script.sh:*)").is_none(),
            "Should allow 'sh -x script.sh' (no -c flag)"
        );
    }

    #[test]
    fn test_extract_shell_interpreter_command() {
        // Basic cases
        assert_eq!(
            extract_shell_interpreter_command("sh -c 'git push'"),
            Some("git push".to_string())
        );
        assert_eq!(
            extract_shell_interpreter_command("bash -c \"git push\""),
            Some("git push".to_string())
        );
        // Combined flags
        assert_eq!(
            extract_shell_interpreter_command("bash -lc 'git push'"),
            Some("git push".to_string())
        );
        // No quotes — only first token after -c is the command string
        // (in POSIX shell, `push` would be $0, not part of the command)
        assert_eq!(
            extract_shell_interpreter_command("sh -c git push"),
            Some("git".to_string())
        );
        // Not a shell interpreter
        assert_eq!(extract_shell_interpreter_command("git push"), None);
        // No -c flag
        assert_eq!(extract_shell_interpreter_command("bash script.sh"), None);
        // Empty
        assert_eq!(extract_shell_interpreter_command(""), None);
        // -c but no command after
        assert_eq!(extract_shell_interpreter_command("sh -c"), None);
        // Absolute path interpreter
        assert_eq!(
            extract_shell_interpreter_command("/bin/sh -c 'git push'"),
            Some("git push".to_string())
        );
        // Wrapper + interpreter
        assert_eq!(
            extract_shell_interpreter_command("env bash -c 'git push'"),
            Some("git push".to_string())
        );
        // Trailing arguments after quoted command string are ignored (they become $0, $1, ...)
        assert_eq!(
            extract_shell_interpreter_command("bash -c \"git push\" x"),
            Some("git push".to_string())
        );
        assert_eq!(
            extract_shell_interpreter_command("sh -c 'git push' --"),
            Some("git push".to_string())
        );
        // Trailing arguments do not pollute extracted command
        assert_eq!(
            extract_shell_interpreter_command("bash -c 'git push origin main' extra args"),
            Some("git push origin main".to_string())
        );
    }
}
