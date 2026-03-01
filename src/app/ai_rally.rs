use anyhow::Result;
use crossterm::event::{self, KeyCode};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use tokio::sync::mpsc;

use crate::ai::orchestrator::{OrchestratorCommand, RallyEvent};
use crate::ai::{Context, Orchestrator, RallyState};
use crate::ui;

use super::types::*;
use super::{App, AppState};

impl App {
    pub(crate) async fn handle_ai_rally_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // Handle modal state first
        if let Some(ref mut rally_state) = self.ai_rally_state {
            if rally_state.showing_log_detail {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                        rally_state.showing_log_detail = false;
                    }
                    _ => {}
                }
                return Ok(());
            }
        }

        match key.code {
            KeyCode::Char('b') => {
                // バックグラウンドで実行を継続したままFileListに戻る
                // abort()を呼ばない、状態も保持したまま
                self.state = AppState::FileList;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                // Send abort command to orchestrator if in waiting state
                if let Some(ref state) = self.ai_rally_state {
                    if matches!(
                        state.state,
                        RallyState::WaitingForClarification
                            | RallyState::WaitingForPermission
                            | RallyState::WaitingForPostConfirmation
                    ) {
                        self.send_rally_command(OrchestratorCommand::Abort);
                    }
                }
                // Abort the orchestrator task if running
                if let Some(handle) = self.rally_abort_handle.take() {
                    handle.abort();
                }
                // Abort rally and return to file list
                self.cleanup_rally_state();
                self.state = AppState::FileList;
            }
            KeyCode::Char('y') => {
                // Grant permission or open clarification editor
                let current_state = self
                    .ai_rally_state
                    .as_ref()
                    .map(|s| s.state)
                    .unwrap_or(RallyState::Error);

                match current_state {
                    RallyState::WaitingForPermission => {
                        // Send permission granted
                        self.send_rally_command(OrchestratorCommand::PermissionResponse(true));
                        // Clear pending permission and update state to prevent duplicate sends
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_permission = None;
                            rally_state.state = RallyState::RevieweeFix;
                            rally_state.push_log(LogEntry::new(
                                LogEventType::Info,
                                "Permission granted, continuing...".to_string(),
                            ));
                        }
                    }
                    RallyState::WaitingForClarification => {
                        // Get the question for the editor
                        let question = self
                            .ai_rally_state
                            .as_ref()
                            .and_then(|s| s.pending_question.clone())
                            .unwrap_or_default();

                        // Open editor synchronously (restore terminal first)
                        self.open_clarification_editor_sync(&question, terminal)?;
                    }
                    RallyState::WaitingForPostConfirmation => {
                        // Approve posting
                        self.send_rally_command(OrchestratorCommand::PostConfirmResponse(true));
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_review_post = None;
                            rally_state.pending_fix_post = None;
                            // Transition state immediately to prevent duplicate sends
                            rally_state.state = RallyState::RevieweeFix;
                            rally_state.push_log(LogEntry::new(
                                LogEventType::Info,
                                "Post approved, posting to PR...".to_string(),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char('n') => {
                // Deny permission or skip clarification
                let current_state = self
                    .ai_rally_state
                    .as_ref()
                    .map(|s| s.state)
                    .unwrap_or(RallyState::Error);

                match current_state {
                    RallyState::WaitingForPermission => {
                        // Send permission denied
                        self.send_rally_command(OrchestratorCommand::PermissionResponse(false));
                        // Clear pending permission - state change is delegated to Orchestrator's StateChanged event
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_permission = None;
                            // Do NOT change rally_state.state here - let Orchestrator's StateChanged event handle it
                            rally_state.push_log(LogEntry::new(
                                LogEventType::Info,
                                "Permission denied, continuing without it...".to_string(),
                            ));
                        }
                    }
                    RallyState::WaitingForClarification => {
                        // Send skip clarification (continue with best judgment)
                        self.send_rally_command(OrchestratorCommand::SkipClarification);
                        // Clear pending question - state change is delegated to Orchestrator's StateChanged event
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_question = None;
                            // Do NOT change rally_state.state here - let Orchestrator's StateChanged event handle it
                            rally_state.push_log(LogEntry::new(
                                LogEventType::Info,
                                "Clarification skipped, continuing with best judgment..."
                                    .to_string(),
                            ));
                        }
                    }
                    RallyState::WaitingForPostConfirmation => {
                        // Skip posting
                        self.send_rally_command(OrchestratorCommand::PostConfirmResponse(false));
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_review_post = None;
                            rally_state.pending_fix_post = None;
                            // Transition state immediately to prevent duplicate sends
                            rally_state.state = RallyState::RevieweeFix;
                            rally_state.push_log(LogEntry::new(
                                LogEventType::Info,
                                "Post skipped, continuing...".to_string(),
                            ));
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char('r') => {
                // Retry on error state
                if let Some(ref state) = self.ai_rally_state {
                    if state.state == RallyState::Error {
                        // Abort current handle if any
                        if let Some(handle) = self.rally_abort_handle.take() {
                            handle.abort();
                        }
                        // Clear state and restart
                        self.ai_rally_state = None;
                        self.rally_event_receiver = None;
                        self.state = AppState::FileList;
                        // Restart the rally
                        self.start_ai_rally();
                    }
                }
            }
            // Log selection and scrolling
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    let total_logs = rally_state.logs.len();
                    if total_logs == 0 {
                        return Ok(());
                    }

                    // Initialize selection if not set
                    let current = rally_state.selected_log_index.unwrap_or(0);
                    let new_index = (current + 1).min(total_logs.saturating_sub(1));
                    rally_state.selected_log_index = Some(new_index);

                    // Auto-scroll to keep selection visible
                    self.adjust_log_scroll_to_selection();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    let total_logs = rally_state.logs.len();
                    if total_logs == 0 {
                        return Ok(());
                    }

                    // Initialize selection if not set (start from last)
                    let current = rally_state
                        .selected_log_index
                        .unwrap_or(total_logs.saturating_sub(1));
                    let new_index = current.saturating_sub(1);
                    rally_state.selected_log_index = Some(new_index);

                    // Auto-scroll to keep selection visible
                    self.adjust_log_scroll_to_selection();
                }
            }
            KeyCode::Char('J') => {
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    let total_logs = rally_state.logs.len();
                    if total_logs == 0 {
                        return Ok(());
                    }

                    let page_step = rally_state.last_visible_log_height.saturating_sub(1).max(1);
                    let current = rally_state.selected_log_index.unwrap_or(0);
                    let new_index = (current + page_step).min(total_logs.saturating_sub(1));
                    rally_state.selected_log_index = Some(new_index);
                    self.adjust_log_scroll_to_selection();
                }
            }
            KeyCode::Char('K') => {
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    let total_logs = rally_state.logs.len();
                    if total_logs == 0 {
                        return Ok(());
                    }

                    let page_step = rally_state.last_visible_log_height.saturating_sub(1).max(1);
                    let current = rally_state
                        .selected_log_index
                        .unwrap_or(total_logs.saturating_sub(1));
                    let new_index = current.saturating_sub(page_step);
                    rally_state.selected_log_index = Some(new_index);
                    self.adjust_log_scroll_to_selection();
                }
            }
            KeyCode::Enter => {
                // Show log detail modal
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    if rally_state.selected_log_index.is_some() && !rally_state.logs.is_empty() {
                        rally_state.showing_log_detail = true;
                    }
                }
            }
            KeyCode::Char('G') => {
                // Jump to bottom
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    let total_logs = rally_state.logs.len();
                    if total_logs > 0 {
                        rally_state.selected_log_index = Some(total_logs.saturating_sub(1));
                        rally_state.log_scroll_offset = 0; // 0 means auto-scroll to bottom
                    }
                }
            }
            KeyCode::Char('g') => {
                // Jump to top
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    if !rally_state.logs.is_empty() {
                        rally_state.selected_log_index = Some(0);
                        rally_state.log_scroll_offset = 1; // 1 is minimum (not 0 which means auto-scroll)
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn adjust_log_scroll_to_selection(&mut self) {
        if let Some(ref mut rally_state) = self.ai_rally_state {
            let Some(selected) = rally_state.selected_log_index else {
                return;
            };

            let visible_height = rally_state.last_visible_log_height;

            // Calculate current scroll position
            let total_logs = rally_state.logs.len();
            let scroll_offset = if rally_state.log_scroll_offset == 0 {
                total_logs.saturating_sub(visible_height)
            } else {
                rally_state.log_scroll_offset
            };

            // Adjust scroll to keep selection visible
            if selected < scroll_offset {
                // Selection is above visible area
                rally_state.log_scroll_offset = selected.max(1);
            } else if selected >= scroll_offset + visible_height {
                // Selection is below visible area
                rally_state.log_scroll_offset = selected.saturating_sub(visible_height - 1).max(1);
            }
        }
    }
    pub(crate) fn send_rally_command(&mut self, cmd: OrchestratorCommand) {
        if let Some(ref sender) = self.rally_command_sender {
            // Use try_send since we're not in an async context
            if sender.try_send(cmd).is_err() {
                // Orchestrator may have terminated, clean up state
                self.cleanup_rally_state();
            }
        }
    }

    /// Clean up rally state when orchestrator terminates or user aborts
    pub(crate) fn cleanup_rally_state(&mut self) {
        self.ai_rally_state = None;
        self.rally_command_sender = None;
        self.rally_event_receiver = None;
        if let Some(handle) = self.rally_abort_handle.take() {
            handle.abort();
        }
    }
    pub(crate) fn open_clarification_editor_sync(
        &mut self,
        question: &str,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // Restore terminal before opening editor
        ui::restore_terminal(terminal)?;

        // Open editor (blocking)
        let answer =
            crate::editor::open_clarification_editor(self.config.editor.as_deref(), question)?;

        // Re-setup terminal after editor closes
        *terminal = ui::setup_terminal()?;

        // Process result
        if let Some(ref mut rally_state) = self.ai_rally_state {
            rally_state.pending_question = None;
        }

        match answer {
            Some(text) if !text.trim().is_empty() => {
                // Send clarification response
                self.send_rally_command(OrchestratorCommand::ClarificationResponse(text.clone()));
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    rally_state.push_log(LogEntry::new(
                        LogEventType::Info,
                        format!("Clarification provided: {}", text),
                    ));
                }
            }
            _ => {
                // User cancelled (empty answer)
                self.send_rally_command(OrchestratorCommand::Abort);
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    rally_state.push_log(LogEntry::new(
                        LogEventType::Info,
                        "Clarification cancelled by user".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
    pub(crate) fn resume_or_start_ai_rally(&mut self) {
        // 既存のRallyがあれば画面遷移のみ（完了/エラー状態でも結果確認のため）
        if self.ai_rally_state.is_some() {
            self.state = AppState::AiRally;
            return;
        }
        // そうでなければ新規Rally開始
        self.start_ai_rally();
    }

    /// バックグラウンドでRallyが実行中かどうか（完了・エラー以外）
    #[allow(dead_code)]
    pub fn is_rally_running_in_background(&self) -> bool {
        self.state != AppState::AiRally
            && self
                .ai_rally_state
                .as_ref()
                .map(|s| s.state.is_active())
                .unwrap_or(false)
    }

    /// バックグラウンドでRallyが存在するかどうか（完了・エラー含む）
    pub fn has_background_rally(&self) -> bool {
        self.state != AppState::AiRally && self.ai_rally_state.is_some()
    }

    /// バックグラウンドRallyが完了またはエラーで終了したかどうか
    #[allow(dead_code)]
    pub fn is_background_rally_finished(&self) -> bool {
        self.state != AppState::AiRally
            && self
                .ai_rally_state
                .as_ref()
                .map(|s| s.state.is_finished())
                .unwrap_or(false)
    }

    pub(crate) fn start_ai_rally(&mut self) {
        // Get PR data for context
        let Some(pr) = self.pr() else {
            return;
        };

        let file_patches: Vec<(String, String)> = self
            .files()
            .iter()
            .filter_map(|f| f.patch.as_ref().map(|p| (f.filename.clone(), p.clone())))
            .collect();

        let diff = file_patches
            .iter()
            .map(|(_, p)| p.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let base_branch = if self.local_mode {
            Self::detect_local_base_branch(self.working_dir.as_deref())
                .unwrap_or_else(|| "main".to_string())
        } else {
            pr.base.ref_name.clone()
        };

        let context = Context {
            repo: self.repo.clone(),
            pr_number: self.pr_number(),
            pr_title: pr.title.clone(),
            pr_body: pr.body.clone(),
            diff,
            working_dir: self.working_dir.clone(),
            head_sha: pr.head.sha.clone(),
            base_branch,
            external_comments: Vec::new(),
            local_mode: self.local_mode,
            file_patches,
        };

        let (event_tx, event_rx) = mpsc::channel(100);
        let (cmd_tx, cmd_rx) = mpsc::channel(10);

        // Store channels first to prevent race conditions
        self.rally_event_receiver = Some(event_rx);
        self.rally_command_sender = Some(cmd_tx);

        // Initialize rally state
        self.ai_rally_state = Some(AiRallyState {
            iteration: 0,
            max_iterations: self.config.ai.max_iterations,
            state: RallyState::Initializing,
            history: Vec::new(),
            logs: Vec::new(),
            log_scroll_offset: 0,
            selected_log_index: None,
            showing_log_detail: false,
            pending_question: None,
            pending_permission: None,
            pending_review_post: None,
            pending_fix_post: None,
            last_visible_log_height: 10,
        });

        self.state = AppState::AiRally;

        // Spawn the orchestrator and store the abort handle
        let config = self.config.ai.clone();
        let repo = self.repo.clone();
        let pr_number = self.pr_number();

        let handle = tokio::spawn(async move {
            let orchestrator_result =
                Orchestrator::new(&repo, pr_number, config, event_tx.clone(), Some(cmd_rx));
            match orchestrator_result {
                Ok(mut orchestrator) => {
                    orchestrator.set_context(context);
                    // Note: orchestrator.run() already emits RallyEvent::Error and
                    // StateChanged(Error) when it fails, so we don't emit them again here
                    // to avoid duplicate error logs in the UI
                    let _ = orchestrator.run().await;
                }
                Err(e) => {
                    // Send error via event channel so it displays in TUI
                    let _ = event_tx
                        .send(RallyEvent::Error(format!(
                            "Failed to create orchestrator: {}",
                            e
                        )))
                        .await;
                }
            }
        });

        // Store the abort handle so we can cancel the task when user presses 'q'
        self.rally_abort_handle = Some(handle.abort_handle());
    }
}
