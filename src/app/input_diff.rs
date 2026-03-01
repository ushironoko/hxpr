use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;

use crate::filter::ListFilter;
use crate::keybinding::{event_to_keybinding, SequenceMatch};

use super::types::*;
use super::{App, AppState};

impl App {
    pub(crate) async fn handle_split_view_file_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // フィルタ入力中はフィルタ処理を優先
        if self.handle_filter_input(&key, "file") {
            // フィルタ操作後に diff プレビューを同期
            self.sync_diff_to_selected_file();
            return Ok(());
        }

        let kb = self.config.keybindings.clone();
        let has_filter = self.file_list_filter.is_some();

        // Move down
        if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
            if has_filter {
                self.handle_filter_navigation("file", true);
            } else if !self.files().is_empty() {
                self.selected_file =
                    (self.selected_file + 1).min(self.files().len().saturating_sub(1));
            }
            self.sync_diff_to_selected_file();
            return Ok(());
        }

        // Move up
        if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
            if has_filter {
                self.handle_filter_navigation("file", false);
            } else if self.selected_file > 0 {
                self.selected_file = self.selected_file.saturating_sub(1);
            }
            self.sync_diff_to_selected_file();
            return Ok(());
        }

        // Page down (Ctrl-d by default, also J)
        if self.matches_single_key(&key, &kb.page_down) || Self::is_shift_char_shortcut(&key, 'j') {
            if !self.files().is_empty() && !has_filter {
                let page_step = terminal.size()?.height.saturating_sub(8) as usize;
                let step = page_step.max(1);
                self.selected_file =
                    (self.selected_file + step).min(self.files().len().saturating_sub(1));
                self.sync_diff_to_selected_file();
            }
            return Ok(());
        }

        // Page up (Ctrl-u by default, also K)
        if self.matches_single_key(&key, &kb.page_up) || Self::is_shift_char_shortcut(&key, 'k') {
            if !has_filter {
                let page_step = terminal.size()?.height.saturating_sub(8) as usize;
                let step = page_step.max(1);
                self.selected_file = self.selected_file.saturating_sub(step);
                self.sync_diff_to_selected_file();
            }
            return Ok(());
        }

        // Esc: フィルタ適用中なら解除、なければ通常動作
        if key.code == KeyCode::Esc {
            if self.handle_filter_esc("file") {
                return Ok(());
            }
            self.state = AppState::FileList;
            return Ok(());
        }

        // Space+/ シーケンス処理（分割表示でのフィルタ起動）
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

        // Focus diff pane
        if self.matches_single_key(&key, &kb.open_panel)
            || self.matches_single_key(&key, &kb.move_right)
            || key.code == KeyCode::Right
        {
            if self.is_filter_selection_empty("file") {
                return Ok(());
            }
            if !self.files().is_empty() {
                self.state = AppState::SplitViewDiff;
            }
            return Ok(());
        }

        // Back to file list
        if self.matches_single_key(&key, &kb.quit)
            || self.matches_single_key(&key, &kb.move_left)
            || key.code == KeyCode::Left
        {
            self.state = AppState::FileList;
            return Ok(());
        }

        // Comment list
        if self.matches_single_key(&key, &kb.comment_list) {
            self.previous_state = AppState::SplitViewFileList;
            self.open_comment_list();
            return Ok(());
        }

        // Help
        if self.matches_single_key(&key, &kb.help) {
            self.previous_state = AppState::SplitViewFileList;
            self.state = AppState::Help;
            self.help_scroll_offset = 0;
            return Ok(());
        }

        // Fallback to common file list keys
        self.handle_common_file_list_keys(key, terminal).await?;

        Ok(())
    }
    pub(super) async fn handle_diff_input_common(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        variant: DiffViewVariant,
    ) -> Result<()> {
        // シンボルポップアップ表示中
        if self.symbol_popup.is_some() {
            self.handle_symbol_popup_input(key, terminal).await?;
            return Ok(());
        }

        let term_size = terminal.size()?;
        let term_h = term_size.height as usize;
        let term_w = term_size.width as usize;

        // Calculate visible_lines based on variant
        let visible_lines = match variant {
            DiffViewVariant::SplitPane => {
                // Header(3) + Footer(3) + border(2) = 8 を差し引き、65%の高さ
                (term_h * 65 / 100).saturating_sub(8)
            }
            DiffViewVariant::Fullscreen => term_h.saturating_sub(8),
        };
        let panel_inner_width = self.comment_panel_inner_width(term_w);

        // Clone keybindings to avoid borrow issues with self
        let kb = self.config.keybindings.clone();

        // 複数行選択モード中
        if self.multiline_selection.is_some() {
            // Move down: カーソルを下に移動
            if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
                if self.diff_line_count > 0 {
                    let new_cursor =
                        (self.selected_line + 1).min(self.diff_line_count.saturating_sub(1));
                    self.selected_line = new_cursor;
                    if let Some(ref mut sel) = self.multiline_selection {
                        sel.cursor_line = new_cursor;
                    }
                    self.adjust_scroll(visible_lines);
                }
                return Ok(());
            }

            // Move up: カーソルを上に移動
            if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
                let new_cursor = self.selected_line.saturating_sub(1);
                self.selected_line = new_cursor;
                if let Some(ref mut sel) = self.multiline_selection {
                    sel.cursor_line = new_cursor;
                }
                self.adjust_scroll(visible_lines);
                return Ok(());
            }

            // c: 選択範囲でコメント入力を開始
            // Enter は使わない — c/s で明示的にコメント/サジェスチョンを選択させる
            if self.matches_single_key(&key, &kb.comment) {
                self.enter_multiline_comment_input();
                return Ok(());
            }

            // s: 選択範囲でサジェスチョン入力を開始
            if self.matches_single_key(&key, &kb.suggestion) {
                self.enter_multiline_suggestion_input();
                return Ok(());
            }

            // Esc / q: 選択モードをキャンセル
            if self.matches_single_key(&key, &kb.quit) || key.code == KeyCode::Esc {
                self.multiline_selection = None;
                return Ok(());
            }

            // その他のキーは無視（選択モード中は限定操作のみ）
            return Ok(());
        }

        // コメントパネルフォーカス中
        if self.comment_panel_open {
            // Move down in panel
            if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
                let max_scroll = self.max_comment_panel_scroll(term_h, term_w);
                self.comment_panel_scroll =
                    self.comment_panel_scroll.saturating_add(1).min(max_scroll);
                return Ok(());
            }

            // Move up in panel
            if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
                self.comment_panel_scroll = self.comment_panel_scroll.saturating_sub(1);
                return Ok(());
            }

            // Next comment
            if self.matches_single_key(&key, &kb.next_comment) {
                let prev_line = self.selected_line;
                self.jump_to_next_comment();
                if self.selected_line != prev_line {
                    self.comment_panel_scroll = 0;
                    self.selected_inline_comment = 0;
                    self.adjust_scroll(visible_lines);
                }
                return Ok(());
            }

            // Previous comment
            if self.matches_single_key(&key, &kb.prev_comment) {
                let prev_line = self.selected_line;
                self.jump_to_prev_comment();
                if self.selected_line != prev_line {
                    self.comment_panel_scroll = 0;
                    self.selected_inline_comment = 0;
                    self.adjust_scroll(visible_lines);
                }
                return Ok(());
            }

            // Add comment
            if self.matches_single_key(&key, &kb.comment) {
                self.enter_comment_input();
                return Ok(());
            }

            // Add suggestion
            if self.matches_single_key(&key, &kb.suggestion) {
                self.enter_suggestion_input();
                return Ok(());
            }

            // Reply
            if self.matches_single_key(&key, &kb.reply) {
                if self.has_comment_at_current_line() {
                    self.enter_reply_input();
                }
                return Ok(());
            }

            // Tab - select next inline comment
            if key.code == KeyCode::Tab {
                if self.has_comment_at_current_line() {
                    let count = self.get_comment_indices_at_current_line().len();
                    if count > 1 && self.selected_inline_comment + 1 < count {
                        self.selected_inline_comment += 1;
                        self.comment_panel_scroll = self.comment_panel_offset_for(
                            self.selected_inline_comment,
                            panel_inner_width,
                        );
                    }
                }
                return Ok(());
            }

            // Shift-Tab - select previous inline comment
            if key.code == KeyCode::BackTab {
                if self.has_comment_at_current_line() {
                    let count = self.get_comment_indices_at_current_line().len();
                    if count > 1 && self.selected_inline_comment > 0 {
                        self.selected_inline_comment -= 1;
                        self.comment_panel_scroll = self.comment_panel_offset_for(
                            self.selected_inline_comment,
                            panel_inner_width,
                        );
                    }
                }
                return Ok(());
            }

            // Variant-specific panel navigation
            match variant {
                DiffViewVariant::SplitPane => {
                    // Go to fullscreen diff
                    if self.matches_single_key(&key, &kb.move_right) || key.code == KeyCode::Right {
                        self.diff_view_return_state = AppState::SplitViewDiff;
                        self.preview_return_state = AppState::DiffView;
                        self.state = AppState::DiffView;
                        return Ok(());
                    }

                    // Close panel
                    if self.matches_single_key(&key, &kb.quit)
                        || self.matches_single_key(&key, &kb.move_left)
                        || key.code == KeyCode::Left
                        || key.code == KeyCode::Esc
                    {
                        self.comment_panel_open = false;
                        self.comment_panel_scroll = 0;
                        return Ok(());
                    }
                }
                DiffViewVariant::Fullscreen => {
                    // Back
                    if self.matches_single_key(&key, &kb.move_left) || key.code == KeyCode::Left {
                        self.state = self.diff_view_return_state;
                        return Ok(());
                    }

                    // Close panel
                    if self.matches_single_key(&key, &kb.quit) || key.code == KeyCode::Esc {
                        self.comment_panel_open = false;
                        self.comment_panel_scroll = 0;
                        return Ok(());
                    }
                }
            }

            return Ok(());
        }

        // Check for sequence timeout
        self.check_sequence_timeout();

        // Get KeyBinding for current event
        let current_kb = event_to_keybinding(&key);

        // Try to match two-key sequences (gd, gf, gg)
        if let Some(kb_event) = current_kb {
            // Check if this key continues a pending sequence
            if !self.pending_keys.is_empty() {
                self.push_pending_key(kb_event);

                // Check for go_to_definition (gd)
                if self.try_match_sequence(&kb.go_to_definition) == SequenceMatch::Full {
                    self.clear_pending_keys();
                    self.open_symbol_popup(terminal).await?;
                    return Ok(());
                }

                // Check for go_to_file (gf)
                if self.try_match_sequence(&kb.go_to_file) == SequenceMatch::Full {
                    self.clear_pending_keys();
                    self.open_current_file_in_editor(terminal).await?;
                    return Ok(());
                }

                // Check for jump_to_first (gg)
                if self.try_match_sequence(&kb.jump_to_first) == SequenceMatch::Full {
                    self.clear_pending_keys();
                    self.selected_line = 0;
                    self.scroll_offset = 0;
                    return Ok(());
                }

                // No match - clear pending keys and fall through
                self.clear_pending_keys();
            } else {
                // Check if this key could start a sequence
                let could_start_gd = self.key_could_match_sequence(&key, &kb.go_to_definition);
                let could_start_gf = self.key_could_match_sequence(&key, &kb.go_to_file);
                let could_start_gg = self.key_could_match_sequence(&key, &kb.jump_to_first);

                if could_start_gd || could_start_gf || could_start_gg {
                    self.push_pending_key(kb_event);
                    return Ok(());
                }
            }
        }

        // Variant-specific quit/back handling (outside panel)
        match variant {
            DiffViewVariant::SplitPane => {
                // Go to fullscreen diff
                if self.matches_single_key(&key, &kb.move_right) || key.code == KeyCode::Right {
                    self.diff_view_return_state = AppState::SplitViewDiff;
                    self.preview_return_state = AppState::DiffView;
                    self.state = AppState::DiffView;
                    return Ok(());
                }

                // Back to file list focus
                if self.matches_single_key(&key, &kb.move_left) || key.code == KeyCode::Left {
                    self.state = AppState::SplitViewFileList;
                    return Ok(());
                }

                // Quit to file list
                if self.matches_single_key(&key, &kb.quit) || key.code == KeyCode::Esc {
                    self.state = AppState::FileList;
                    return Ok(());
                }

                // Add comment (without panel)
                if self.matches_single_key(&key, &kb.comment) {
                    self.enter_comment_input();
                    return Ok(());
                }

                // Add suggestion (without panel)
                if self.matches_single_key(&key, &kb.suggestion) {
                    self.enter_suggestion_input();
                    return Ok(());
                }
            }
            DiffViewVariant::Fullscreen => {
                // Quit/back
                if self.matches_single_key(&key, &kb.quit) || key.code == KeyCode::Esc {
                    // If started from PR list and we're at the file list level, go back to PR list
                    if self.started_from_pr_list
                        && self.diff_view_return_state == AppState::FileList
                    {
                        self.back_to_pr_list();
                    } else {
                        self.state = self.diff_view_return_state;
                    }
                    return Ok(());
                }

                // Back (left arrow or h) - goes to file list, not PR list
                if self.matches_single_key(&key, &kb.move_left) || key.code == KeyCode::Left {
                    self.state = self.diff_view_return_state;
                    return Ok(());
                }
            }
        }

        // Common single-key bindings

        // Shift+Enter or fallback key (V): 複数行選択モードに入る
        if (key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::SHIFT))
            || self.matches_single_key(&key, &kb.multiline_select)
        {
            self.enter_multiline_selection();
            return Ok(());
        }

        // Move down
        if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
            if self.diff_line_count > 0 {
                self.selected_line =
                    (self.selected_line + 1).min(self.diff_line_count.saturating_sub(1));
                self.adjust_scroll(visible_lines);
            }
            return Ok(());
        }

        // Move up
        if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
            self.selected_line = self.selected_line.saturating_sub(1);
            self.adjust_scroll(visible_lines);
            return Ok(());
        }

        // Jump to last
        if self.matches_single_key(&key, &kb.jump_to_last) {
            if self.diff_line_count > 0 {
                self.selected_line = self.diff_line_count.saturating_sub(1);
                self.adjust_scroll(visible_lines);
            }
            return Ok(());
        }

        // Jump back
        if self.matches_single_key(&key, &kb.jump_back) {
            self.jump_back();
            return Ok(());
        }

        // Page down
        if self.matches_single_key(&key, &kb.page_down) || Self::is_shift_char_shortcut(&key, 'j') {
            if self.diff_line_count > 0 {
                self.selected_line =
                    (self.selected_line + 20).min(self.diff_line_count.saturating_sub(1));
                self.adjust_scroll(visible_lines);
            }
            return Ok(());
        }

        // Page up
        if self.matches_single_key(&key, &kb.page_up) || Self::is_shift_char_shortcut(&key, 'k') {
            self.selected_line = self.selected_line.saturating_sub(20);
            self.adjust_scroll(visible_lines);
            return Ok(());
        }

        // Next comment
        if self.matches_single_key(&key, &kb.next_comment) {
            self.jump_to_next_comment();
            return Ok(());
        }

        // Previous comment
        if self.matches_single_key(&key, &kb.prev_comment) {
            self.jump_to_prev_comment();
            return Ok(());
        }

        // Toggle markdown rich display
        if self.matches_single_key(&key, &kb.toggle_markdown_rich) {
            self.toggle_markdown_rich();
            self.ensure_diff_cache();
            return Ok(());
        }

        // Open panel (local mode ではコメント対象の PR がないため無効)
        if !self.local_mode && self.matches_single_key(&key, &kb.open_panel) {
            self.comment_panel_open = true;
            self.comment_panel_scroll = 0;
            self.selected_inline_comment = 0;
            return Ok(());
        }

        // Fullscreen-only: Add comment (without panel)
        if variant == DiffViewVariant::Fullscreen {
            if self.matches_single_key(&key, &kb.comment) {
                self.enter_comment_input();
                return Ok(());
            }

            // Add suggestion (without panel)
            if self.matches_single_key(&key, &kb.suggestion) {
                self.enter_suggestion_input();
                return Ok(());
            }
        }

        Ok(())
    }
    pub(crate) async fn handle_split_view_diff_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        self.handle_diff_input_common(key, terminal, DiffViewVariant::SplitPane)
            .await
    }
    pub(crate) async fn handle_diff_view_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        self.handle_diff_input_common(key, terminal, DiffViewVariant::Fullscreen)
            .await
    }
    pub(crate) fn adjust_scroll(&mut self, visible_lines: usize) {
        if visible_lines == 0 {
            return;
        }
        if self.selected_line < self.scroll_offset {
            self.scroll_offset = self.selected_line;
        }
        if self.selected_line >= self.scroll_offset + visible_lines {
            self.scroll_offset = self.selected_line.saturating_sub(visible_lines) + 1;
        }

        // Allow additional scrolling when at the end (bottom padding)
        // This enables showing empty space below the last line
        let padding = visible_lines / 2;
        let max_scroll_with_padding = self.diff_line_count.saturating_sub(1);
        if self.selected_line >= self.diff_line_count.saturating_sub(padding) {
            // When near the end, allow scroll_offset to go further
            let target_scroll = self.selected_line.saturating_sub(visible_lines / 2);
            self.scroll_offset = target_scroll.min(max_scroll_with_padding);
        }
    }
}
