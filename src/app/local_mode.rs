use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use notify::Watcher;

use crate::cache::PrCacheKey;

use super::types::*;
use super::App;

impl App {
    pub(crate) fn toggle_markdown_rich(&mut self) {
        self.markdown_rich = !self.markdown_rich;

        // 現在のファイルがmarkdownならキャッシュを無効化
        let current_is_md = self
            .files()
            .get(self.selected_file)
            .map(|f| crate::language::is_markdown_ext_from_filename(&f.filename))
            .unwrap_or(false);

        if current_is_md {
            self.diff_cache = None;
            self.diff_cache_receiver = None;
        }

        // ストア内のmarkdownファイルのキャッシュのみ無効化
        let files = self.files();
        let md_indices: Vec<usize> = self
            .highlighted_cache_store
            .keys()
            .copied()
            .filter(|idx| {
                files
                    .get(*idx)
                    .map(|f| crate::language::is_markdown_ext_from_filename(&f.filename))
                    .unwrap_or(false)
            })
            .collect();
        for idx in md_indices {
            self.highlighted_cache_store.remove(&idx);
        }

        // プリフェッチも停止（markdown_richフラグが変わったため再構築が必要）
        self.prefetch_receiver = None;
    }

    pub(crate) fn toggle_local_mode(&mut self) {
        // フォアグラウンド Rally 中はブロック
        if matches!(self.state, AppState::AiRally) {
            self.submission_result =
                Some((false, "Cannot toggle mode during AI Rally".to_string()));
            self.submission_result_time = Some(Instant::now());
            return;
        }

        // PR モードの in-flight viewed mutation を破棄
        self.mark_viewed_receiver = None;
        // Local モードの in-flight バッチ/lazy diff を破棄（クロスPRキャッシュ汚染防止）
        self.batch_diff_receiver = None;
        self.lazy_diff_receiver = None;
        self.lazy_diff_pending_file = None;

        if self.local_mode {
            // Local → PR
            self.deactivate_watcher();
            self.saved_local_snapshot = Some(self.save_view_snapshot());
            self.local_mode = false;
            // モード切替時にファイルフィルタをリセット（stale indices による OOB 防止）
            self.file_list_filter = None;

            if let Some(snapshot) = self.saved_pr_snapshot.take() {
                let pr_number = snapshot.pr_number;
                self.restore_view_snapshot(snapshot);

                // data_receiver の origin_pr を更新
                if let Some(pr) = pr_number {
                    self.update_data_receiver_origin(pr);
                }

                // SessionCache からデータ復元
                self.restore_data_from_cache();
            } else if let Some(pr) = self.original_pr_number {
                // original_pr_number で復帰
                self.pr_number = Some(pr);
                self.update_data_receiver_origin(pr);
                self.restore_data_from_cache();
            } else if self.started_from_pr_list {
                self.back_to_pr_list();
            } else {
                // 復帰先がない → local に戻してエラー表示
                self.local_mode = true;
                self.saved_local_snapshot = None; // 戻す
                if let Some(handle) = &self.watcher_handle {
                    handle.active.store(true, Ordering::Release);
                }
                self.submission_result = Some((false, "No PR to return to".to_string()));
                self.submission_result_time = Some(Instant::now());
                return;
            }

            self.submission_result = Some((true, "Switched to PR mode".to_string()));
        } else {
            // PR → Local
            let from_pr_list = matches!(self.state, AppState::PullRequestList);
            self.saved_pr_snapshot = Some(self.save_view_snapshot());
            self.local_mode = true;
            // モード切替時にファイルフィルタをリセット（stale indices による OOB 防止）
            self.file_list_filter = None;

            // PR リストから来た場合は FileList に遷移
            if from_pr_list {
                self.state = AppState::FileList;
            }

            if let Some(snapshot) = self.saved_local_snapshot.take() {
                self.restore_view_snapshot(snapshot);
            } else {
                // 初回: ビューリセット
                self.selected_file = 0;
                self.file_list_scroll_offset = 0;
                self.selected_line = 0;
                self.scroll_offset = 0;
                self.diff_cache = None;
                self.highlighted_cache_store.clear();
                self.review_comments = None;
                self.discussion_comments = None;
            }

            // restore_view_snapshot がスナップショットの pr_number で上書きする可能性があるため、
            // Local モードでは常に 0 を強制
            self.pr_number = Some(0);

            // data_receiver の origin_pr を 0 (local) に更新
            self.update_data_receiver_origin(0);
            // stale な in-flight view 系 receiver をクリア
            self.diff_cache_receiver = None;
            self.prefetch_receiver = None;

            // SessionCache からデータ復元
            let cache_key = PrCacheKey {
                repo: self.repo.clone(),
                pr_number: 0,
            };
            if let Some(cached) = self.session_cache.get_pr_data(&cache_key) {
                self.data_state = DataState::Loaded {
                    pr: cached.pr.clone(),
                    files: cached.files.clone(),
                };
                self.diff_line_count =
                    Self::calc_diff_line_count(&cached.files, self.selected_file);
                self.start_prefetch_all_files();
            } else {
                self.data_state = DataState::Loading;
            }

            self.activate_watcher();
            // 常にバックグラウンドで最新データを取得
            self.retry_load();

            self.submission_result = Some((true, "Switched to Local mode".to_string()));
        }

        self.submission_result_time = Some(Instant::now());
    }

    /// data_receiver の origin_pr を更新（channel 自体は再作成しない）
    pub(crate) fn update_data_receiver_origin(&mut self, pr_number: u32) {
        if let Some((ref mut origin, _)) = self.data_receiver {
            *origin = pr_number;
        }
    }

    /// SessionCache からデータを復元し、ない場合は Loading + retry_load
    pub(crate) fn restore_data_from_cache(&mut self) {
        let pr_number = self.pr_number.unwrap_or(0);
        let cache_key = PrCacheKey {
            repo: self.repo.clone(),
            pr_number,
        };
        if let Some(cached) = self.session_cache.get_pr_data(&cache_key) {
            self.data_state = DataState::Loaded {
                pr: cached.pr.clone(),
                files: cached.files.clone(),
            };
            self.diff_line_count = Self::calc_diff_line_count(&cached.files, self.selected_file);
            self.start_prefetch_all_files();
        } else {
            self.data_state = DataState::Loading;
        }
        // 常にバックグラウンドで最新データを取得
        self.retry_load();
    }

    /// ローカルブランチのベースブランチを検出
    pub(crate) fn detect_local_base_branch(working_dir: Option<&str>) -> Option<String> {
        let mut cmd = std::process::Command::new("git");
        cmd.args(["rev-parse", "--abbrev-ref", "@{upstream}"]);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let upstream = String::from_utf8_lossy(&output.stdout).trim().to_string();
                // "origin/main" → "main"
                if let Some(branch) = upstream.strip_prefix("origin/") {
                    return Some(branch.to_string());
                }
                return Some(upstream);
            }
        }

        // Fallback: origin/main or origin/master が存在するか確認
        for candidate in &["main", "master"] {
            let mut cmd = std::process::Command::new("git");
            cmd.args(["rev-parse", "--verify", &format!("origin/{}", candidate)]);
            if let Some(dir) = working_dir {
                cmd.current_dir(dir);
            }
            if let Ok(output) = cmd.output() {
                if output.status.success() {
                    return Some(candidate.to_string());
                }
            }
        }

        None
    }

    /// 現在のビュー状態をスナップショットとして保存（O(1) 移動）
    ///
    /// データは `SessionCache` に格納済みのため、`data_state` は保存しない。
    pub(crate) fn save_view_snapshot(&mut self) -> ViewSnapshot {
        ViewSnapshot {
            pr_number: self.pr_number,
            selected_file: self.selected_file,
            file_list_scroll_offset: self.file_list_scroll_offset,
            selected_line: self.selected_line,
            scroll_offset: self.scroll_offset,
            diff_cache: self.diff_cache.take(),
            highlighted_cache_store: std::mem::take(&mut self.highlighted_cache_store),
            review_comments: self.review_comments.take(),
            discussion_comments: self.discussion_comments.take(),
            local_file_signatures: std::mem::take(&mut self.local_file_signatures),
            local_file_patch_signatures: std::mem::take(&mut self.local_file_patch_signatures),
        }
    }

    /// スナップショットから UI 状態を復元（O(1) 移動）
    ///
    /// channel は触らない（永続チャンネルのため）。
    /// データは `SessionCache` から別途取得する。
    pub(crate) fn restore_view_snapshot(&mut self, snapshot: ViewSnapshot) {
        self.pr_number = snapshot.pr_number;
        self.selected_file = snapshot.selected_file;
        self.file_list_scroll_offset = snapshot.file_list_scroll_offset;
        self.selected_line = snapshot.selected_line;
        self.scroll_offset = snapshot.scroll_offset;
        self.diff_cache = snapshot.diff_cache;
        self.highlighted_cache_store = snapshot.highlighted_cache_store;
        self.review_comments = snapshot.review_comments;
        self.discussion_comments = snapshot.discussion_comments;
        self.local_file_signatures = snapshot.local_file_signatures;
        self.local_file_patch_signatures = snapshot.local_file_patch_signatures;

        // stale な in-flight view 系 receiver をクリア
        self.diff_cache_receiver = None;
        self.prefetch_receiver = None;
        self.comment_receiver = None;
        self.discussion_comment_receiver = None;
        self.comment_submit_receiver = None;
        self.comment_submitting = false;
        self.comments_loading = false;
        self.discussion_comments_loading = false;
    }

    /// ファイルウォッチャーを有効化（初回は作成、2回目以降は active フラグを ON）
    pub(crate) fn activate_watcher(&mut self) {
        if let Some(ref handle) = self.watcher_handle {
            handle.active.store(true, Ordering::Release);
            return;
        }

        // retry_sender が必要
        let Some(ref retry_sender) = self.retry_sender else {
            return;
        };

        let refresh_pending = self
            .refresh_pending
            .get_or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();

        let watch_dir = self.working_dir.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });

        let active = Arc::new(AtomicBool::new(true));
        let active_clone = active.clone();
        let refresh_tx = retry_sender.clone();

        let thread = std::thread::spawn(move || {
            let callback = move |result: notify::Result<notify::Event>| {
                if !active_clone.load(Ordering::Acquire) {
                    return;
                }

                let Ok(event) = result else {
                    return;
                };

                let dominated_by_git = event
                    .paths
                    .iter()
                    .all(|p| p.components().any(|c| c.as_os_str() == ".git"));
                let is_access = matches!(event.kind, notify::EventKind::Access(_));

                if !is_access && !dominated_by_git && !refresh_pending.swap(true, Ordering::AcqRel)
                {
                    let _ = refresh_tx.try_send(RefreshRequest::LocalRefresh);
                }
            };

            let Ok(mut watcher) =
                notify::RecommendedWatcher::new(callback, notify::Config::default())
            else {
                return;
            };

            let _ = watcher.watch(
                std::path::Path::new(&watch_dir),
                notify::RecursiveMode::Recursive,
            );

            loop {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
        });

        self.watcher_handle = Some(WatcherHandle {
            active,
            _thread: thread,
        });
    }

    /// ファイルウォッチャーを無効化（active フラグを OFF）
    pub(crate) fn deactivate_watcher(&mut self) {
        if let Some(ref handle) = self.watcher_handle {
            handle.active.store(false, Ordering::Release);
        }
    }

    pub(crate) fn toggle_auto_focus(&mut self) {
        self.local_auto_focus = !self.local_auto_focus;
        let msg = if self.local_auto_focus {
            "Auto-focus: ON"
        } else {
            "Auto-focus: OFF"
        };
        self.submission_result = Some((true, msg.to_string()));
        self.submission_result_time = Some(Instant::now());
    }
}
