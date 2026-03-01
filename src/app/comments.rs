use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::cache::PrCacheKey;
use crate::github::{self, comment::ReviewComment};
use crate::ui;

use super::types::*;
use super::{App, AppState};

impl App {
    pub(crate) fn enter_comment_input(&mut self) {
        if self.local_mode {
            return;
        }
        let Some(file) = self.files().get(self.selected_file) else {
            return;
        };
        let Some(patch) = file.patch.as_ref() else {
            return;
        };

        // Get actual line number from diff
        let Some(line_info) = crate::diff::get_line_info(patch, self.selected_line) else {
            return;
        };

        // Only allow comments on Added or Context lines (not Removed/Header/Meta)
        if !matches!(
            line_info.line_type,
            crate::diff::LineType::Added | crate::diff::LineType::Context
        ) {
            return;
        }

        let Some(line_number) = line_info.new_line_number else {
            return;
        };

        let Some(diff_position) = line_info.diff_position else {
            return;
        };

        self.input_mode = Some(InputMode::Comment(LineInputContext {
            file_index: self.selected_file,
            line_number,
            diff_position,
            start_line_number: None,
        }));
        self.input_text_area.clear();
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }
    pub(crate) async fn submit_review(
        &mut self,
        action: ReviewAction,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        tracing::debug!(?action, "submit_review: start");
        ui::restore_terminal(terminal)?;

        let editor_result = crate::editor::open_review_editor(self.config.editor.as_deref());
        tracing::debug!(?editor_result, "submit_review: editor returned");

        // エディタの成否に関わらずターミナルを再セットアップ
        *terminal = ui::setup_terminal()?;

        let body = match editor_result {
            Ok(body) => body,
            Err(e) => {
                tracing::debug!(%e, "submit_review: editor failed");
                self.submission_result = Some((false, format!("Editor failed: {}", e)));
                self.submission_result_time = Some(Instant::now());
                return Ok(());
            }
        };

        let Some(body) = body else {
            tracing::debug!("submit_review: body is None");
            if action == ReviewAction::Approve {
                // Empty comment → show approve confirmation UI
                self.pending_approve_body = Some(String::new());
            } else {
                self.submission_result = Some((false, "Review cancelled".to_string()));
                self.submission_result_time = Some(Instant::now());
            }
            return Ok(());
        };

        // All actions with non-empty body: submit immediately
        self.submit_review_with_body(action, &body).await
    }

    pub(crate) async fn submit_review_with_body(&mut self, action: ReviewAction, body: &str) -> Result<()> {
        tracing::debug!(body_len = body.len(), "submit_review: calling GitHub API");
        match github::submit_review(&self.repo, self.pr_number(), action, body).await {
            Ok(()) => {
                let action_str = match action {
                    ReviewAction::Approve => "approved",
                    ReviewAction::RequestChanges => "changes requested",
                    ReviewAction::Comment => "commented",
                };
                tracing::debug!(action_str, "submit_review: success");
                self.submission_result = Some((true, format!("Review submitted ({})", action_str)));
                self.submission_result_time = Some(Instant::now());
            }
            Err(e) => {
                tracing::debug!(%e, "submit_review: API failed");
                self.submission_result = Some((false, format!("Review failed: {}", e)));
                self.submission_result_time = Some(Instant::now());
            }
        }
        self.pending_approve_body = None;
        Ok(())
    }
    pub(crate) fn enter_suggestion_input(&mut self) {
        if self.local_mode {
            return;
        }
        let Some(file) = self.files().get(self.selected_file) else {
            return;
        };
        let Some(patch) = file.patch.as_ref() else {
            return;
        };

        // Check if this line can have a suggestion
        let Some(line_info) = crate::diff::get_line_info(patch, self.selected_line) else {
            return;
        };

        // Only allow suggestions on Added or Context lines
        if !matches!(
            line_info.line_type,
            crate::diff::LineType::Added | crate::diff::LineType::Context
        ) {
            return;
        }

        let Some(line_number) = line_info.new_line_number else {
            return;
        };

        let Some(diff_position) = line_info.diff_position else {
            return;
        };

        let original_code = line_info.line_content.clone();

        self.input_mode = Some(InputMode::Suggestion {
            context: LineInputContext {
                file_index: self.selected_file,
                line_number,
                diff_position,
                start_line_number: None,
            },
            original_code: original_code.clone(),
        });
        // サジェスチョンは元コードを初期値として設定
        self.input_text_area.set_content(&original_code);
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }
    /// 複数行選択モードを開始する（Shift+Enter）
    pub(crate) fn enter_multiline_selection(&mut self) {
        if self.local_mode {
            return;
        }
        // 現在の行がコメント可能な行であることを確認
        let Some(file) = self.files().get(self.selected_file) else {
            return;
        };
        let Some(patch) = file.patch.as_ref() else {
            return;
        };
        let Some(line_info) = crate::diff::get_line_info(patch, self.selected_line) else {
            return;
        };
        if !matches!(
            line_info.line_type,
            crate::diff::LineType::Added | crate::diff::LineType::Context
        ) {
            return;
        }
        self.multiline_selection = Some(MultilineSelection {
            anchor_line: self.selected_line,
            cursor_line: self.selected_line,
        });
    }
    pub(crate) fn enter_multiline_comment_input(&mut self) {
        if self.local_mode {
            return;
        }
        let Some(ref selection) = self.multiline_selection else {
            return;
        };
        let start = selection.start();
        let end = selection.end();

        let Some(file) = self.files().get(self.selected_file) else {
            return;
        };
        let Some(patch) = file.patch.as_ref() else {
            return;
        };

        // 範囲内の全行が同一ハンク内の new-side 行であることを検証
        if !crate::diff::validate_multiline_range(patch, start, end) {
            return;
        }

        // 終了行の情報を取得（GitHub API の line パラメータ）
        let Some(end_info) = crate::diff::get_line_info(patch, end) else {
            return;
        };
        if !matches!(
            end_info.line_type,
            crate::diff::LineType::Added | crate::diff::LineType::Context
        ) {
            return;
        }
        let Some(end_line_number) = end_info.new_line_number else {
            return;
        };
        let Some(diff_position) = end_info.diff_position else {
            return;
        };

        // 開始行の情報を取得（GitHub API の start_line パラメータ）
        let Some(start_info) = crate::diff::get_line_info(patch, start) else {
            return;
        };
        let Some(start_line_number) = start_info.new_line_number else {
            return;
        };

        // 単一行の場合は start_line_number を None にする
        let start_line = if start_line_number < end_line_number {
            Some(start_line_number)
        } else {
            None
        };

        // バリデーション成功後にのみ選択状態をクリア
        self.multiline_selection = None;

        self.input_mode = Some(InputMode::Comment(LineInputContext {
            file_index: self.selected_file,
            line_number: end_line_number,
            diff_position,
            start_line_number: start_line,
        }));
        self.input_text_area.clear();
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }
    pub(crate) fn enter_multiline_suggestion_input(&mut self) {
        if self.local_mode {
            return;
        }
        let Some(ref selection) = self.multiline_selection else {
            return;
        };
        let start = selection.start();
        let end = selection.end();

        let Some(file) = self.files().get(self.selected_file) else {
            return;
        };
        let Some(patch) = file.patch.as_ref() else {
            return;
        };

        // 範囲内の全行が同一ハンク内の new-side 行であることを検証
        if !crate::diff::validate_multiline_range(patch, start, end) {
            return;
        }

        // 終了行の情報を取得
        let Some(end_info) = crate::diff::get_line_info(patch, end) else {
            return;
        };
        if !matches!(
            end_info.line_type,
            crate::diff::LineType::Added | crate::diff::LineType::Context
        ) {
            return;
        }
        let Some(end_line_number) = end_info.new_line_number else {
            return;
        };
        let Some(diff_position) = end_info.diff_position else {
            return;
        };

        // 開始行の情報を取得
        let Some(start_info) = crate::diff::get_line_info(patch, start) else {
            return;
        };
        let Some(start_line_number) = start_info.new_line_number else {
            return;
        };

        // 選択範囲のコードを収集
        let mut original_lines = Vec::new();
        for line_idx in start..=end {
            if let Some(info) = crate::diff::get_line_info(patch, line_idx) {
                if matches!(
                    info.line_type,
                    crate::diff::LineType::Added | crate::diff::LineType::Context
                ) {
                    original_lines.push(info.line_content.clone());
                }
            }
        }
        let original_code = original_lines.join("\n");

        let start_line = if start_line_number < end_line_number {
            Some(start_line_number)
        } else {
            None
        };

        // バリデーション成功後にのみ選択状態をクリア
        self.multiline_selection = None;

        self.input_mode = Some(InputMode::Suggestion {
            context: LineInputContext {
                file_index: self.selected_file,
                line_number: end_line_number,
                diff_position,
                start_line_number: start_line,
            },
            original_code: original_code.clone(),
        });
        self.input_text_area.set_content(&original_code);
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }
    pub(crate) fn open_comment_list(&mut self) {
        if self.local_mode {
            return;
        }
        self.state = AppState::CommentList;
        self.discussion_comment_detail_mode = false;
        self.discussion_comment_detail_scroll = 0;

        // Load review comments
        self.load_review_comments();
        // Load discussion comments
        self.load_discussion_comments();
    }

    pub(crate) fn load_review_comments(&mut self) {
        let cache_key = PrCacheKey {
            repo: self.repo.clone(),
            pr_number: self.pr_number(),
        };

        // インメモリキャッシュを確認
        if let Some(comments) = self.session_cache.get_review_comments(&cache_key) {
            self.review_comments = Some(comments.to_vec());
            self.selected_comment = 0;
            self.comment_list_scroll_offset = 0;
            self.comments_loading = false;
            return;
        }

        // キャッシュミス: API取得
        self.comments_loading = true;
        let (tx, rx) = mpsc::channel(1);
        let pr_number = self.pr_number();
        self.comment_receiver = Some((pr_number, rx));

        let repo = self.repo.clone();

        tokio::spawn(async move {
            // Fetch both review comments and reviews
            let review_comments_result =
                github::comment::fetch_review_comments(&repo, pr_number).await;
            let reviews_result = github::comment::fetch_reviews(&repo, pr_number).await;

            // Combine results
            let mut all_comments: Vec<ReviewComment> = Vec::new();

            // Add review comments (inline comments)
            if let Ok(comments) = review_comments_result {
                all_comments.extend(comments);
            }

            // Convert reviews to ReviewComment format (only those with body)
            if let Ok(reviews) = reviews_result {
                for review in reviews {
                    if let Some(body) = review.body {
                        if !body.trim().is_empty() {
                            all_comments.push(ReviewComment {
                                id: review.id,
                                path: "[PR Review]".to_string(),
                                line: None,
                                body,
                                user: review.user,
                                created_at: review.submitted_at.unwrap_or_default(),
                            });
                        }
                    }
                }
            }

            // Sort by created_at
            all_comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));

            let _ = tx.send(Ok(all_comments)).await;
        });
    }

    pub(crate) fn load_discussion_comments(&mut self) {
        let cache_key = PrCacheKey {
            repo: self.repo.clone(),
            pr_number: self.pr_number(),
        };

        // インメモリキャッシュを確認
        if let Some(comments) = self.session_cache.get_discussion_comments(&cache_key) {
            self.discussion_comments = Some(comments.to_vec());
            self.selected_discussion_comment = 0;
            self.discussion_comments_loading = false;
            return;
        }

        // キャッシュミス: API取得
        self.discussion_comments_loading = true;
        let (tx, rx) = mpsc::channel(1);
        let pr_number = self.pr_number();
        self.discussion_comment_receiver = Some((pr_number, rx));

        let repo = self.repo.clone();

        tokio::spawn(async move {
            match github::comment::fetch_discussion_comments(&repo, pr_number).await {
                Ok(comments) => {
                    let _ = tx.send(Ok(comments)).await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string())).await;
                }
            }
        });
    }
    pub(crate) async fn handle_comment_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let visible_lines = terminal.size()?.height.saturating_sub(8) as usize;

        // Handle detail mode input separately
        if self.discussion_comment_detail_mode {
            return self.handle_discussion_detail_input(key, visible_lines);
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state = self.previous_state;
            }
            KeyCode::Char('[') => {
                self.comment_tab = match self.comment_tab {
                    CommentTab::Review => CommentTab::Discussion,
                    CommentTab::Discussion => CommentTab::Review,
                };
            }
            KeyCode::Char(']') => {
                self.comment_tab = match self.comment_tab {
                    CommentTab::Review => CommentTab::Discussion,
                    CommentTab::Discussion => CommentTab::Review,
                };
            }
            KeyCode::Char('j') | KeyCode::Down => match self.comment_tab {
                CommentTab::Review => {
                    if let Some(ref comments) = self.review_comments {
                        if !comments.is_empty() {
                            self.selected_comment =
                                (self.selected_comment + 1).min(comments.len().saturating_sub(1));
                        }
                    }
                }
                CommentTab::Discussion => {
                    if let Some(ref comments) = self.discussion_comments {
                        if !comments.is_empty() {
                            self.selected_discussion_comment = (self.selected_discussion_comment
                                + 1)
                            .min(comments.len().saturating_sub(1));
                        }
                    }
                }
            },
            KeyCode::Char('k') | KeyCode::Up => match self.comment_tab {
                CommentTab::Review => {
                    self.selected_comment = self.selected_comment.saturating_sub(1);
                }
                CommentTab::Discussion => {
                    self.selected_discussion_comment =
                        self.selected_discussion_comment.saturating_sub(1);
                }
            },
            KeyCode::Char('J') => {
                let step = visible_lines.max(1);
                match self.comment_tab {
                    CommentTab::Review => {
                        if let Some(ref comments) = self.review_comments {
                            if !comments.is_empty() {
                                self.selected_comment =
                                    (self.selected_comment + step).min(comments.len() - 1);
                            }
                        }
                    }
                    CommentTab::Discussion => {
                        if let Some(ref comments) = self.discussion_comments {
                            if !comments.is_empty() {
                                self.selected_discussion_comment =
                                    (self.selected_discussion_comment + step)
                                        .min(comments.len() - 1);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('K') => {
                let step = visible_lines.max(1);
                match self.comment_tab {
                    CommentTab::Review => {
                        self.selected_comment = self.selected_comment.saturating_sub(step);
                    }
                    CommentTab::Discussion => {
                        self.selected_discussion_comment =
                            self.selected_discussion_comment.saturating_sub(step);
                    }
                }
            }
            KeyCode::Enter => match self.comment_tab {
                CommentTab::Review => {
                    self.jump_to_comment();
                }
                CommentTab::Discussion => {
                    // Enter detail mode for discussion comment
                    if self
                        .discussion_comments
                        .as_ref()
                        .map(|c| !c.is_empty())
                        .unwrap_or(false)
                    {
                        self.discussion_comment_detail_mode = true;
                        self.discussion_comment_detail_scroll = 0;
                    }
                }
            },
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_discussion_detail_input(
        &mut self,
        key: event::KeyEvent,
        visible_lines: usize,
    ) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                self.discussion_comment_detail_mode = false;
                self.discussion_comment_detail_scroll = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.discussion_comment_detail_scroll =
                    self.discussion_comment_detail_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.discussion_comment_detail_scroll =
                    self.discussion_comment_detail_scroll.saturating_sub(1);
            }
            KeyCode::Char('J') => {
                self.discussion_comment_detail_scroll = self
                    .discussion_comment_detail_scroll
                    .saturating_add(visible_lines.max(1));
            }
            KeyCode::Char('K') => {
                self.discussion_comment_detail_scroll = self
                    .discussion_comment_detail_scroll
                    .saturating_sub(visible_lines.max(1));
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.discussion_comment_detail_scroll = self
                    .discussion_comment_detail_scroll
                    .saturating_add(visible_lines / 2);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.discussion_comment_detail_scroll = self
                    .discussion_comment_detail_scroll
                    .saturating_sub(visible_lines / 2);
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn jump_to_comment(&mut self) {
        let Some(ref comments) = self.review_comments else {
            return;
        };
        let Some(comment) = comments.get(self.selected_comment) else {
            return;
        };

        let target_path = &comment.path;

        // Find file index by path
        let file_index = self.files().iter().position(|f| &f.filename == target_path);

        if let Some(idx) = file_index {
            self.selected_file = idx;
            self.diff_view_return_state = AppState::FileList;
            self.state = AppState::DiffView;
            self.selected_line = 0;
            self.scroll_offset = 0;
            self.update_diff_line_count();
            self.update_file_comment_positions();
            self.ensure_diff_cache();

            // Find diff line index from pre-computed positions
            let diff_line_index = self
                .file_comment_positions
                .iter()
                .find(|pos| pos.comment_index == self.selected_comment)
                .map(|pos| pos.diff_line_index);

            if let Some(line_idx) = diff_line_index {
                self.selected_line = line_idx;
                self.scroll_offset = line_idx;
            }
        }
    }

    /// Update file_comment_positions based on current file and review_comments
    pub(crate) fn update_file_comment_positions(&mut self) {
        self.file_comment_positions.clear();
        self.file_comment_lines.clear();

        let Some(file) = self.files().get(self.selected_file) else {
            return;
        };
        let Some(patch) = file.patch.clone() else {
            return;
        };
        let filename = file.filename.clone();

        let Some(ref comments) = self.review_comments else {
            return;
        };

        for (i, comment) in comments.iter().enumerate() {
            // Skip comments for other files
            if comment.path != filename {
                continue;
            }
            // Skip PR-level comments (line: None)
            let Some(line_num) = comment.line else {
                continue;
            };
            if let Some(diff_index) = Self::find_diff_line_index(&patch, line_num) {
                self.file_comment_positions.push(CommentPosition {
                    diff_line_index: diff_index,
                    comment_index: i,
                });
                self.file_comment_lines.insert(diff_index);
            }
        }
        self.file_comment_positions
            .sort_by_key(|pos| pos.diff_line_index);
    }

    /// Static helper to find diff line index for a given line number
    pub(crate) fn find_diff_line_index(patch: &str, target_line: u32) -> Option<usize> {
        let lines: Vec<&str> = patch.lines().collect();
        let mut new_line_number: Option<u32> = None;

        for (i, line) in lines.iter().enumerate() {
            if line.starts_with("@@") {
                // Parse hunk header to get starting line number
                if let Some(plus_pos) = line.find('+') {
                    let after_plus = &line[plus_pos + 1..];
                    let end_pos = after_plus.find([',', ' ']).unwrap_or(after_plus.len());
                    if let Ok(num) = after_plus[..end_pos].parse::<u32>() {
                        new_line_number = Some(num);
                    }
                }
            } else if line.starts_with('+') || line.starts_with(' ') {
                if let Some(current) = new_line_number {
                    if current == target_line {
                        return Some(i);
                    }
                    new_line_number = Some(current + 1);
                }
            }
            // Removed lines don't increment new_line_number
        }

        None
    }
    /// Get comment indices at the current selected line
    pub fn get_comment_indices_at_current_line(&self) -> Vec<usize> {
        self.file_comment_positions
            .iter()
            .filter(|pos| pos.diff_line_index == self.selected_line)
            .map(|pos| pos.comment_index)
            .collect()
    }

    /// Check if current line has any comments
    pub fn has_comment_at_current_line(&self) -> bool {
        self.file_comment_positions
            .iter()
            .any(|pos| pos.diff_line_index == self.selected_line)
    }

    /// テキスト行がパネル幅内で折り返される表示行数を計算
    pub(crate) fn wrapped_line_count(text: &str, panel_width: usize) -> usize {
        if panel_width == 0 {
            return 1;
        }
        let char_count = text.chars().count();
        if char_count == 0 {
            return 1;
        }
        char_count.div_ceil(panel_width)
    }

    /// コメント本文の折り返しを考慮した表示行数を計算
    pub(crate) fn comment_body_wrapped_lines(body: &str, panel_width: usize) -> usize {
        body.lines()
            .map(|line| Self::wrapped_line_count(line, panel_width))
            .sum::<usize>()
            .max(1) // 空の本文でも最低1行
    }

    /// コメントパネルのコンテンツ行数を計算（スクロール上限算出用）
    pub(crate) fn comment_panel_content_lines(&self, panel_inner_width: usize) -> usize {
        let indices = self.get_comment_indices_at_current_line();
        if indices.is_empty() {
            return 1; // "No comments..." message
        }
        let Some(ref comments) = self.review_comments else {
            return 0;
        };
        let mut count = 0usize;
        for (i, &idx) in indices.iter().enumerate() {
            let Some(comment) = comments.get(idx) else {
                continue;
            };
            if i > 0 {
                count += 1; // separator
            }
            count += 1; // header
            count += Self::comment_body_wrapped_lines(&comment.body, panel_inner_width);
            count += 1; // spacing
        }
        count
    }

    /// 指定インラインコメントのパネル内行オフセットを計算（スクロール追従用）
    pub(crate) fn comment_panel_offset_for(&self, target: usize, panel_inner_width: usize) -> u16 {
        let indices = self.get_comment_indices_at_current_line();
        let Some(ref comments) = self.review_comments else {
            return 0;
        };
        let mut offset = 0usize;
        for (i, &idx) in indices.iter().enumerate() {
            if i == target {
                break;
            }
            let Some(comment) = comments.get(idx) else {
                continue;
            };
            if i > 0 {
                offset += 1; // separator
            }
            offset += 1; // header
            offset += Self::comment_body_wrapped_lines(&comment.body, panel_inner_width);
            offset += 1; // spacing
        }
        if target > 0 {
            offset += 1; // separator before target
        }
        offset as u16
    }

    /// コメントパネルの内側幅を計算（borders分の2を差し引く）
    pub(crate) fn comment_panel_inner_width(&self, terminal_width: usize) -> usize {
        let panel_width = match self.state {
            AppState::SplitViewDiff => terminal_width * 65 / 100,
            _ => terminal_width,
        };
        panel_width.saturating_sub(2) // borders
    }

    /// コメントパネルのスクロール上限を計算
    pub(crate) fn max_comment_panel_scroll(&self, terminal_height: usize, terminal_width: usize) -> u16 {
        let panel_inner_width = self.comment_panel_inner_width(terminal_width);
        let content_lines = self.comment_panel_content_lines(panel_inner_width);
        // コメントパネルは全体高さの約40%（Header/Footer/borders分を差し引き）
        let panel_inner_height = (terminal_height.saturating_sub(8) * 40 / 100).max(1);
        content_lines.saturating_sub(panel_inner_height) as u16
    }
    pub(crate) fn jump_to_next_comment(&mut self) {
        let next = self
            .file_comment_positions
            .iter()
            .find(|pos| pos.diff_line_index > self.selected_line);

        if let Some(pos) = next {
            self.selected_line = pos.diff_line_index;
            self.scroll_offset = self.selected_line;
        }
    }

    /// Jump to previous comment in the diff (no wrap-around, scroll to top)
    pub(crate) fn jump_to_prev_comment(&mut self) {
        let prev = self
            .file_comment_positions
            .iter()
            .rev()
            .find(|pos| pos.diff_line_index < self.selected_line);

        if let Some(pos) = prev {
            self.selected_line = pos.diff_line_index;
            self.scroll_offset = self.selected_line;
        }
    }
    pub(crate) fn enter_reply_input(&mut self) {
        let indices = self.get_comment_indices_at_current_line();
        if indices.is_empty() {
            return;
        }

        let local_idx = self
            .selected_inline_comment
            .min(indices.len().saturating_sub(1));
        let comment_idx = indices[local_idx];

        let Some(ref comments) = self.review_comments else {
            return;
        };
        let Some(comment) = comments.get(comment_idx) else {
            return;
        };

        self.input_mode = Some(InputMode::Reply {
            comment_id: comment.id,
            reply_to_user: comment.user.login.clone(),
            reply_to_body: comment.body.clone(),
        });
        self.input_text_area.clear();
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }
}
