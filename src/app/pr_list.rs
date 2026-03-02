use anyhow::Result;
use crossterm::event::{self, KeyCode};
use tokio::sync::mpsc;

use crate::cache::PrCacheKey;
use crate::filter::ListFilter;
use crate::github::{self, PrStateFilter};
use crate::keybinding::{event_to_keybinding, SequenceMatch};

use super::{App, AppState, DataState};

impl App {
    pub(crate) async fn handle_pr_list_input(&mut self, key: event::KeyEvent) -> Result<()> {
        // Clone keybindings to avoid borrow conflicts
        let kb = self.config.keybindings.clone();

        // フィルタ入力中はフィルタ処理を優先
        if self.handle_filter_input(&key, "pr") {
            return Ok(());
        }

        // Quit
        if self.matches_single_key(&key, &kb.quit) {
            self.should_quit = true;
            return Ok(());
        }

        // ローディング中は操作を受け付けない（quitは上で処理済み）
        if self.pr_list_loading {
            return Ok(());
        }

        let pr_count = self.pr_list.as_ref().map(|l| l.len()).unwrap_or(0);
        let has_filter = self.pr_list_filter.is_some();

        // Move down (j or Down arrow)
        if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
            if has_filter {
                self.handle_filter_navigation("pr", true);
            } else if pr_count > 0 {
                self.selected_pr = (self.selected_pr + 1).min(pr_count.saturating_sub(1));
                // 無限スクロール: 残り5件で次を取得
                if self.pr_list_has_more
                    && !self.pr_list_loading
                    && self.selected_pr + 5 >= pr_count
                {
                    self.load_more_prs();
                }
            }
            return Ok(());
        }

        // Move up (k or Up arrow)
        if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
            if has_filter {
                self.handle_filter_navigation("pr", false);
            } else {
                self.selected_pr = self.selected_pr.saturating_sub(1);
            }
            return Ok(());
        }

        // Page down (Ctrl-d by default, also J)
        if self.matches_single_key(&key, &kb.page_down) || Self::is_shift_char_shortcut(&key, 'j') {
            if pr_count > 0 && !has_filter {
                let step = 20usize;
                self.selected_pr = (self.selected_pr + step).min(pr_count.saturating_sub(1));
                if self.pr_list_has_more
                    && !self.pr_list_loading
                    && self.selected_pr + 5 >= pr_count
                {
                    self.load_more_prs();
                }
            }
            return Ok(());
        }

        // Page up (Ctrl-u by default, also K)
        if self.matches_single_key(&key, &kb.page_up) || Self::is_shift_char_shortcut(&key, 'k') {
            if !has_filter {
                self.selected_pr = self.selected_pr.saturating_sub(20);
            }
            return Ok(());
        }

        // Esc: フィルタ適用中なら解除
        if key.code == KeyCode::Esc && self.handle_filter_esc("pr") {
            return Ok(());
        }

        // gg/G/Space+/ シーケンス処理
        if let Some(kb_event) = event_to_keybinding(&key) {
            self.check_sequence_timeout();

            if !self.pending_keys.is_empty() {
                self.push_pending_key(kb_event);

                // gg: 先頭へ（フィルタ適用中は無効化）
                if self.try_match_sequence(&kb.jump_to_first) == SequenceMatch::Full {
                    self.clear_pending_keys();
                    if !has_filter {
                        self.selected_pr = 0;
                    }
                    return Ok(());
                }

                // Space+/: フィルタ起動
                if self.try_match_sequence(&kb.filter) == SequenceMatch::Full {
                    self.clear_pending_keys();
                    if let Some(ref mut filter) = self.pr_list_filter {
                        // 既存フィルタを再編集
                        filter.input_active = true;
                    } else {
                        let mut filter = ListFilter::new();
                        // 初期状態で全アイテムをマッチ
                        if let Some(prs) = self.pr_list.as_ref() {
                            filter.apply(prs, |_pr, _q| true);
                            if let Some(idx) = filter.sync_selection() {
                                self.selected_pr = idx;
                            }
                        }
                        self.pr_list_filter = Some(filter);
                    }
                    return Ok(());
                }

                // マッチしなければペンディングをクリア
                self.clear_pending_keys();
            } else {
                // シーケンス開始チェック
                if self.key_could_match_sequence(&key, &kb.jump_to_first)
                    || self.key_could_match_sequence(&key, &kb.filter)
                {
                    self.push_pending_key(kb_event);
                    return Ok(());
                }
            }
        }

        // G: 末尾へ
        if self.matches_single_key(&key, &kb.jump_to_last) {
            if pr_count > 0 && !has_filter {
                self.selected_pr = pr_count.saturating_sub(1);
            }
            return Ok(());
        }

        // Enter: PR選択
        if self.matches_single_key(&key, &kb.open_panel) {
            if self.is_filter_selection_empty("pr") {
                return Ok(());
            }
            if let Some(ref prs) = self.pr_list {
                if let Some(pr) = prs.get(self.selected_pr) {
                    self.select_pr(pr.number);
                }
            }
            return Ok(());
        }

        // ブラウザで開く（configurable、フィルターキーより先に評価）
        if self.matches_single_key(&key, &kb.open_in_browser) {
            if self.is_filter_selection_empty("pr") {
                return Ok(());
            }
            if let Some(ref prs) = self.pr_list {
                if let Some(pr) = prs.get(self.selected_pr) {
                    self.open_pr_in_browser(pr.number);
                }
            }
            return Ok(());
        }

        // o: open PRのみ
        if key.code == KeyCode::Char('o') {
            if self.pr_list_state_filter != PrStateFilter::Open {
                self.pr_list_state_filter = PrStateFilter::Open;
                self.reload_pr_list();
            }
            return Ok(());
        }

        // c: closed PRのみ
        if key.code == KeyCode::Char('c') {
            if self.pr_list_state_filter != PrStateFilter::Closed {
                self.pr_list_state_filter = PrStateFilter::Closed;
                self.reload_pr_list();
            }
            return Ok(());
        }

        // a: all PRs
        if key.code == KeyCode::Char('a') {
            if self.pr_list_state_filter != PrStateFilter::All {
                self.pr_list_state_filter = PrStateFilter::All;
                self.reload_pr_list();
            }
            return Ok(());
        }

        // r: リフレッシュ
        if self.matches_single_key(&key, &kb.refresh) {
            self.reload_pr_list();
            return Ok(());
        }

        // Toggle local mode
        if self.matches_single_key(&key, &kb.toggle_local_mode) {
            self.toggle_local_mode();
            return Ok(());
        }

        // ?: ヘルプ
        if self.matches_single_key(&key, &kb.help) {
            self.previous_state = AppState::PullRequestList;
            self.state = AppState::Help;
            self.help_scroll_offset = 0;
            self.config_scroll_offset = 0;
            return Ok(());
        }

        Ok(())
    }
    pub(crate) fn reload_pr_list(&mut self) {
        // 既存のリストをクリアせず、ローディング状態のみ設定
        // これにより、ローディング中も既存のリストが表示される
        self.selected_pr = 0;
        self.pr_list_scroll_offset = 0;
        self.pr_list_loading = true;
        self.pr_list_has_more = false;
        self.pr_list_filter = None;

        let (tx, rx) = mpsc::channel(2);
        self.pr_list_receiver = Some(rx);

        let repo = self.repo.clone();
        let state = self.pr_list_state_filter;

        tokio::spawn(async move {
            let result = github::fetch_pr_list(&repo, state, 30).await;
            let _ = tx.send(result.map_err(|e| e.to_string())).await;
        });
    }

    /// 追加のPRを読み込み（無限スクロール用）
    pub(crate) fn load_more_prs(&mut self) {
        if self.pr_list_loading {
            return;
        }

        let offset = self.pr_list.as_ref().map(|l| l.len()).unwrap_or(0) as u32;

        self.pr_list_loading = true;

        let (tx, rx) = mpsc::channel(2);
        self.pr_list_receiver = Some(rx);

        let repo = self.repo.clone();
        let state = self.pr_list_state_filter;

        tokio::spawn(async move {
            let result = github::fetch_pr_list_with_offset(&repo, state, offset, 30).await;
            let _ = tx.send(result.map_err(|e| e.to_string())).await;
        });
    }
    pub(crate) fn select_pr(&mut self, pr_number: u32) {
        self.pr_number = Some(pr_number);
        self.state = AppState::FileList;
        self.file_list_filter = None;
        self.pending_approve_body = None;

        // PR遷移時にバックグラウンドキャッシュをクリア（staleキャッシュ防止）
        self.diff_cache_receiver = None;
        self.prefetch_receiver = None;
        self.mark_viewed_receiver = None;
        self.batch_diff_receiver = None;
        self.lazy_diff_receiver = None;
        self.lazy_diff_pending_file = None;
        self.highlighted_cache_store.clear();
        self.diff_cache = None;
        self.selected_file = 0;
        self.file_list_scroll_offset = 0;

        // Apply pending AI Rally flag
        if self.pending_ai_rally {
            self.start_ai_rally_on_load = true;
        }

        // data_receiver の origin_pr を更新（channel 自体は再作成しない）
        self.update_data_receiver_origin(pr_number);

        // インメモリキャッシュを確認し、Hit/Missに応じて分岐
        let cache_key = PrCacheKey {
            repo: self.repo.clone(),
            pr_number,
        };
        if let Some(cached) = self.session_cache.get_pr_data(&cache_key) {
            let diff_line_count = Self::calc_diff_line_count(&cached.files, 0);
            self.data_state = DataState::Loaded {
                pr: cached.pr.clone(),
                files: cached.files.clone(),
            };
            self.diff_line_count = diff_line_count;
            self.start_prefetch_all_files();
            // キャッシュHit時はhandle_data_resultを経由しないため、ここでRally起動
            if self.start_ai_rally_on_load {
                self.start_ai_rally_on_load = false;
                self.start_ai_rally();
            }
        } else {
            self.data_state = DataState::Loading;
        }

        // 永続リトライループ経由で fetch 開始
        self.retry_load();
    }
    pub fn back_to_pr_list(&mut self) {
        if self.started_from_pr_list {
            // Local モードから戻る場合はスナップショット保存 + watcher 停止
            if self.local_mode {
                self.saved_local_snapshot = Some(self.save_view_snapshot());
                self.deactivate_watcher();
                self.local_mode = false;
            }

            // PR固有の状態をリセット
            self.pr_number = None;
            self.data_state = DataState::Loading;
            self.review_comments = None;
            self.discussion_comments = None;
            self.diff_cache = None;
            // in-flight view 系レシーバーをクリア（late response による panic 防止）
            // data_receiver / retry_sender は永続のため維持
            self.comment_receiver = None;
            self.diff_cache_receiver = None;
            self.prefetch_receiver = None;
            self.discussion_comment_receiver = None;
            self.comment_submit_receiver = None;
            self.mark_viewed_receiver = None;
            self.batch_diff_receiver = None;
            self.lazy_diff_receiver = None;
            self.lazy_diff_pending_file = None;
            self.comment_submitting = false;
            self.pending_approve_body = None;
            self.comments_loading = false;
            self.discussion_comments_loading = false;
            self.highlighted_cache_store.clear();
            self.selected_file = 0;
            self.file_list_scroll_offset = 0;
            self.selected_line = 0;
            self.scroll_offset = 0;
            self.file_list_filter = None;

            self.state = AppState::PullRequestList;
        }
    }
}
