use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::filter::ListFilter;
use crate::github::{self, ChangedFile};
use crate::keybinding::{event_to_keybinding, SequenceMatch};

use super::types::*;
use super::{App, AppState, DataState};

impl App {
    pub(crate) async fn handle_input(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Kitty keyboard protocol が有効な場合、Release/Repeat イベントも
                // 報告されるため、Press のみ処理して二重実行を防止する。
                if key.kind != KeyEventKind::Press {
                    return Ok(());
                }

                // PR一覧画面は独自のLoading処理があるためスキップ
                // Help画面はデータ状態に依存しないためスキップ
                if self.state != AppState::PullRequestList && self.state != AppState::Help {
                    // Error状態でのリトライ処理
                    if let DataState::Error(_) = &self.data_state {
                        match key.code {
                            KeyCode::Char('q') => self.should_quit = true,
                            KeyCode::Char('r') => self.retry_load(),
                            _ => {}
                        }
                        return Ok(());
                    }

                    // Loading状態ではqのみ受け付け
                    if matches!(self.data_state, DataState::Loading) {
                        if key.code == KeyCode::Char('q') {
                            self.should_quit = true;
                        }
                        return Ok(());
                    }

                    if self.pending_approve_body.is_some() {
                        match self.handle_pending_approve_choice(&key) {
                            PendingApproveChoice::Submit => {
                                let body = self.pending_approve_body.take().unwrap_or_default();
                                self.submit_review_with_body(ReviewAction::Approve, &body)
                                    .await?;
                            }
                            PendingApproveChoice::Cancel | PendingApproveChoice::Ignore => {}
                        }
                        return Ok(());
                    }
                }

                match self.state {
                    AppState::PullRequestList => self.handle_pr_list_input(key).await?,
                    AppState::FileList => self.handle_file_list_input(key, terminal).await?,
                    AppState::DiffView => self.handle_diff_view_input(key, terminal).await?,
                    AppState::TextInput => self.handle_text_input(key)?,
                    AppState::CommentList => self.handle_comment_list_input(key, terminal).await?,
                    AppState::Help => self.handle_help_input(key, terminal)?,
                    AppState::AiRally => self.handle_ai_rally_input(key, terminal).await?,
                    AppState::SplitViewFileList => {
                        self.handle_split_view_file_list_input(key, terminal)
                            .await?
                    }
                    AppState::SplitViewDiff => {
                        self.handle_split_view_diff_input(key, terminal).await?
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn retry_load(&mut self) {
        if let Some(ref tx) = self.retry_sender {
            // 既にデータがある場合は Loading に戻さない（バックグラウンド更新のみ）
            if !matches!(self.data_state, DataState::Loaded { .. }) {
                self.data_state = DataState::Loading;
            }
            let request = if self.local_mode {
                RefreshRequest::LocalRefresh
            } else {
                RefreshRequest::PrRefresh {
                    pr_number: self.pr_number.unwrap_or(0),
                }
            };
            let _ = tx.try_send(request);
        }
    }
    pub(crate) async fn handle_file_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // フィルタ入力中はフィルタ処理を優先
        if self.handle_filter_input(&key, "file") {
            return Ok(());
        }

        // フィルタ結果が空の場合、ファイル操作を無効化（stale selection 防止）
        if !self.is_filter_selection_empty("file") && self.handle_mark_viewed_key(key) {
            return Ok(());
        }

        let kb = self.config.keybindings.clone();
        let has_filter = self.file_list_filter.is_some();

        // Quit or back to PR list
        if self.matches_single_key(&key, &kb.quit) {
            if self.started_from_pr_list {
                self.back_to_pr_list();
            } else {
                self.should_quit = true;
            }
            return Ok(());
        }

        // Esc: フィルタ適用中なら解除
        if key.code == KeyCode::Esc && self.handle_filter_esc("file") {
            return Ok(());
        }

        // Move down (j or Down arrow - arrows always work)
        if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
            if has_filter {
                self.handle_filter_navigation("file", true);
            } else if !self.files().is_empty() {
                self.selected_file =
                    (self.selected_file + 1).min(self.files().len().saturating_sub(1));
            }
            return Ok(());
        }

        // Move up (k or Up arrow)
        if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
            if has_filter {
                self.handle_filter_navigation("file", false);
            } else {
                self.selected_file = self.selected_file.saturating_sub(1);
            }
            return Ok(());
        }

        // Page down (Ctrl-d by default, also J)
        if self.matches_single_key(&key, &kb.page_down) || Self::is_shift_char_shortcut(&key, 'j') {
            if !self.files().is_empty() && !has_filter {
                let page_step = terminal.size()?.height.saturating_sub(8) as usize;
                let step = page_step.max(1);
                self.selected_file =
                    (self.selected_file + step).min(self.files().len().saturating_sub(1));
            }
            return Ok(());
        }

        // Page up (Ctrl-u by default, also K)
        if self.matches_single_key(&key, &kb.page_up) || Self::is_shift_char_shortcut(&key, 'k') {
            if !has_filter {
                let page_step = terminal.size()?.height.saturating_sub(8) as usize;
                let step = page_step.max(1);
                self.selected_file = self.selected_file.saturating_sub(step);
            }
            return Ok(());
        }

        // Space+/ シーケンス処理（ファイル一覧でのフィルタ起動）
        if let Some(kb_event) = event_to_keybinding(&key) {
            self.check_sequence_timeout();

            if !self.pending_keys.is_empty() {
                self.push_pending_key(kb_event);

                // Space+/: フィルタ起動
                if self.try_match_sequence(&kb.filter) == SequenceMatch::Full {
                    self.clear_pending_keys();
                    if let Some(ref mut filter) = self.file_list_filter {
                        filter.input_active = true;
                    } else {
                        let mut filter = ListFilter::new();
                        let files = self.files();
                        filter.apply(files, |_file, _q| true);
                        if let Some(idx) = filter.sync_selection() {
                            self.selected_file = idx;
                        }
                        self.file_list_filter = Some(filter);
                    }
                    return Ok(());
                }

                // マッチしなければペンディングをクリア
                self.clear_pending_keys();
            } else {
                // シーケンス開始チェック
                if self.key_could_match_sequence(&key, &kb.filter) {
                    self.push_pending_key(kb_event);
                    return Ok(());
                }
            }
        }

        // Open split view (Enter, Right arrow, or l)
        if self.matches_single_key(&key, &kb.open_panel)
            || self.matches_single_key(&key, &kb.move_right)
            || key.code == KeyCode::Right
        {
            if self.is_filter_selection_empty("file") {
                return Ok(());
            }
            if !self.files().is_empty() {
                self.state = AppState::SplitViewDiff;
                self.sync_diff_to_selected_file();
            }
            return Ok(());
        }

        // Actions (disabled in local mode - no PR to submit reviews to)
        if !self.local_mode && self.matches_single_key(&key, &kb.approve) {
            self.submit_review(ReviewAction::Approve, terminal).await?;
            return Ok(());
        }

        if !self.local_mode && self.matches_single_key(&key, &kb.request_changes) {
            self.submit_review(ReviewAction::RequestChanges, terminal)
                .await?;
            return Ok(());
        }

        // Note: In FileList, 'comment' key triggers review comment (not inline comment)
        // Using separate check for review comment in FileList context
        if !self.local_mode && self.matches_single_key(&key, &kb.comment) {
            self.submit_review(ReviewAction::Comment, terminal).await?;
            return Ok(());
        }

        // Comment list
        if self.matches_single_key(&key, &kb.comment_list) {
            self.previous_state = AppState::FileList;
            self.open_comment_list();
            return Ok(());
        }

        // Refresh
        if self.matches_single_key(&key, &kb.refresh) {
            self.refresh_all();
            return Ok(());
        }

        // AI Rally — ローカルdiffモードでも新規起動・resumeの両方を許可する（仕様）。
        // ローカルモードではコメント投稿等のAPI呼び出しはオーケストレーター側でスキップされる。
        if self.matches_single_key(&key, &kb.ai_rally) {
            self.resume_or_start_ai_rally();
            return Ok(());
        }

        // Open in browser (disabled in local mode)
        if !self.local_mode && self.matches_single_key(&key, &kb.open_in_browser) {
            if let Some(pr_number) = self.pr_number {
                self.open_pr_in_browser(pr_number);
            }
            return Ok(());
        }

        // Toggle local mode
        if self.matches_single_key(&key, &kb.toggle_local_mode) {
            self.toggle_local_mode();
            return Ok(());
        }

        // Toggle auto-focus (local mode only)
        if self.matches_single_key(&key, &kb.toggle_auto_focus) {
            if self.local_mode {
                self.toggle_auto_focus();
            }
            return Ok(());
        }

        // Help
        if self.matches_single_key(&key, &kb.help) {
            self.previous_state = AppState::FileList;
            self.state = AppState::Help;
            self.help_scroll_offset = 0;
            return Ok(());
        }

        Ok(())
    }
    pub(crate) async fn handle_common_file_list_keys(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<bool> {
        // フィルタ結果が空の場合、ファイル操作を無効化（stale selection 防止）
        if !self.is_filter_selection_empty("file") && self.handle_mark_viewed_key(key) {
            return Ok(true);
        }

        let kb = &self.config.keybindings;

        // Review actions (disabled in local mode)
        if !self.local_mode && self.matches_single_key(&key, &kb.approve) {
            self.submit_review(ReviewAction::Approve, terminal).await?;
            return Ok(true);
        }

        if !self.local_mode && self.matches_single_key(&key, &kb.request_changes) {
            self.submit_review(ReviewAction::RequestChanges, terminal)
                .await?;
            return Ok(true);
        }

        if !self.local_mode && self.matches_single_key(&key, &kb.comment) {
            self.submit_review(ReviewAction::Comment, terminal).await?;
            return Ok(true);
        }

        if self.matches_single_key(&key, &kb.refresh) {
            self.refresh_all();
            return Ok(true);
        }

        // AI Rally — ローカルdiffモードでも新規起動・resumeの両方を許可する（仕様）。
        if self.matches_single_key(&key, &kb.ai_rally) {
            self.resume_or_start_ai_rally();
            return Ok(true);
        }

        if !self.local_mode && self.matches_single_key(&key, &kb.open_in_browser) {
            if let Some(pr_number) = self.pr_number {
                self.open_pr_in_browser(pr_number);
            }
            return Ok(true);
        }

        // Toggle local mode
        if self.matches_single_key(&key, &kb.toggle_local_mode) {
            self.toggle_local_mode();
            return Ok(true);
        }

        // Toggle auto-focus (local mode only)
        if self.matches_single_key(&key, &kb.toggle_auto_focus) {
            if self.local_mode {
                self.toggle_auto_focus();
            }
            return Ok(true);
        }

        Ok(false)
    }
    pub(crate) fn handle_mark_viewed_key(&mut self, key: event::KeyEvent) -> bool {
        if self.local_mode {
            return false;
        }

        let is_mark_file = key.code == KeyCode::Char('v')
            && !key.modifiers.contains(KeyModifiers::SHIFT)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        let is_mark_directory = key.code == KeyCode::Char('V')
            || (key.code == KeyCode::Char('v') && key.modifiers.contains(KeyModifiers::SHIFT));
        let has_unexpected_modifiers = key.modifiers.contains(KeyModifiers::CONTROL)
            || key.modifiers.contains(KeyModifiers::ALT);

        if has_unexpected_modifiers || (!is_mark_file && !is_mark_directory) {
            return false;
        }

        if self.mark_viewed_receiver.is_some() {
            self.submission_result = Some((false, "Mark viewed already in progress".to_string()));
            self.submission_result_time = Some(Instant::now());
            return true;
        }

        if is_mark_file {
            self.start_mark_selected_file_as_viewed();
            return true;
        }

        self.start_mark_selected_directory_as_viewed();
        true
    }

    pub(crate) fn start_mark_selected_file_as_viewed(&mut self) {
        let Some(file) = self.files().get(self.selected_file) else {
            return;
        };
        let set_viewed = !file.viewed;

        self.start_mark_paths_as_viewed(vec![file.filename.clone()], set_viewed);
    }

    pub(crate) fn start_mark_selected_directory_as_viewed(&mut self) {
        let target_paths = Self::collect_unviewed_directory_paths(self.files(), self.selected_file);

        if target_paths.is_empty() {
            self.submission_result = Some((true, "No unviewed files in directory".to_string()));
            self.submission_result_time = Some(Instant::now());
            return;
        }

        self.start_mark_paths_as_viewed(target_paths, true);
    }

    pub(crate) fn start_mark_paths_as_viewed(&mut self, paths: Vec<String>, set_viewed: bool) {
        let total_targets = paths.len();
        if total_targets == 0 {
            return;
        }

        let Some(pr_number) = self.pr_number else {
            self.submission_result = Some((false, "PR number not set".to_string()));
            self.submission_result_time = Some(Instant::now());
            return;
        };
        let Some(pr) = self.pr() else {
            self.submission_result = Some((false, "PR metadata not loaded".to_string()));
            self.submission_result_time = Some(Instant::now());
            return;
        };
        let Some(pr_node_id) = pr.node_id.clone() else {
            self.submission_result = Some((false, "PR node ID is unavailable".to_string()));
            self.submission_result_time = Some(Instant::now());
            return;
        };

        let repo = self.repo.clone();
        let (tx, rx) = mpsc::channel(1);
        self.mark_viewed_receiver = Some((pr_number, rx));
        let action_label = if set_viewed { "viewed" } else { "unviewed" };
        self.submission_result = Some((
            true,
            format!("Marking {} file(s) as {}...", total_targets, action_label),
        ));
        self.submission_result_time = Some(Instant::now());

        tokio::spawn(async move {
            let mut marked_paths = Vec::with_capacity(total_targets);
            let mut error = None;

            for path in paths {
                let result = if set_viewed {
                    github::mark_file_as_viewed(&repo, &pr_node_id, &path).await
                } else {
                    github::unmark_file_as_viewed(&repo, &pr_node_id, &path).await
                };
                match result {
                    Ok(()) => marked_paths.push(path),
                    Err(e) => {
                        error = Some(format!("{}: {}", path, e));
                        break;
                    }
                }
            }

            let _ = tx
                .send(MarkViewedResult::Completed {
                    marked_paths,
                    total_targets,
                    error,
                    set_viewed,
                })
                .await;
        });
    }

    pub(crate) fn directory_prefix_for(path: &str) -> String {
        path.rsplit_once('/')
            .map(|(dir, _)| format!("{}/", dir))
            .unwrap_or_default()
    }

    pub(crate) fn collect_unviewed_directory_paths(
        files: &[ChangedFile],
        selected_file: usize,
    ) -> Vec<String> {
        let Some(selected) = files.get(selected_file) else {
            return Vec::new();
        };
        let directory_prefix = Self::directory_prefix_for(&selected.filename);

        files
            .iter()
            .filter(|file| {
                let in_scope = if directory_prefix.is_empty() {
                    !file.filename.contains('/')
                } else {
                    file.filename.starts_with(&directory_prefix)
                };
                in_scope && !file.viewed
            })
            .map(|file| file.filename.clone())
            .collect()
    }
    pub(crate) fn refresh_all(&mut self) {
        // インメモリキャッシュを全削除
        self.session_cache.invalidate_all();
        // コメントデータをクリア
        self.review_comments = None;
        self.discussion_comments = None;
        self.comments_loading = false;
        self.discussion_comments_loading = false;
        self.file_list_filter = None;
        // 強制的に Loading 状態にしてから再取得
        self.data_state = DataState::Loading;
        self.retry_load();
    }

    pub(crate) fn open_pr_in_browser(&self, pr_number: u32) {
        let repo = self.repo.clone();
        tokio::spawn(async move {
            let _ =
                github::gh_command(&["pr", "view", &pr_number.to_string(), "-R", &repo, "--web"])
                    .await;
        });
    }
}
