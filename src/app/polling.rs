use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::ai::orchestrator::RallyEvent;
use crate::ai::RallyState;
use crate::cache::{PrCacheKey, PrData};
use crate::github::ChangedFile;
use crate::loader::{CommentSubmitResult, DataLoadResult};
use crate::syntax::ParserPool;

use super::types::*;
use super::{App, DataState, MAX_HIGHLIGHTED_CACHE_ENTRIES, MAX_PREFETCH_FILES};

impl App {
    pub(crate) fn poll_pr_list_updates(&mut self) {
        let Some(ref mut rx) = self.pr_list_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(page)) => {
                // pr_list_scroll_offset が 0 ならリフレッシュ/フィルタ変更なので置き換え
                // そうでなければ追加ロード
                if self.pr_list_scroll_offset == 0 && self.selected_pr == 0 {
                    // フィルタ変更やリフレッシュ: リストを置き換え
                    self.pr_list = Some(page.items);
                } else if let Some(ref mut existing) = self.pr_list {
                    // 追加ロード: 既存リストに追加
                    existing.extend(page.items);
                } else {
                    // 初回ロード
                    self.pr_list = Some(page.items);
                }
                self.pr_list_has_more = page.has_more;
                self.pr_list_loading = false;
                self.pr_list_receiver = None;

                // フィルタが有効な場合、新データに対してフィルタを再適用
                if self.pr_list_filter.as_ref().is_some_and(|f| f.has_query()) {
                    self.reapply_filter("pr");
                }
            }
            Ok(Err(e)) => {
                eprintln!("Warning: Failed to fetch PR list: {}", e);
                if self.pr_list.is_none() {
                    self.pr_list = Some(vec![]);
                }
                self.pr_list_loading = false;
                self.pr_list_receiver = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                if self.pr_list.is_none() {
                    self.pr_list = Some(vec![]);
                }
                self.pr_list_loading = false;
                self.pr_list_receiver = None;
            }
        }
    }

    /// バックグラウンドタスクからのデータ更新をポーリング
    pub(crate) fn poll_data_updates(&mut self) {
        let Some((_origin_pr, rx)) = self.data_receiver.as_mut() else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => {
                // メッセージ自体から発信元PR番号を取得（mutable な origin_pr に依存しない）
                let source_pr = match &result {
                    DataLoadResult::Success { pr, .. } => Some(pr.number),
                    DataLoadResult::Error(_) => None,
                };

                if source_pr == self.pr_number || source_pr.is_none() {
                    // 現在のPR/モードに一致 → UI状態に反映
                    let pr_number = self.pr_number.unwrap_or(0);
                    self.handle_data_result(pr_number, result);
                } else if let DataLoadResult::Success { pr, files } = result {
                    // 異なるPRのデータ: セッションキャッシュにのみ格納
                    // receiver は破棄しない（永続チャンネルを維持）
                    let cache_key = PrCacheKey {
                        repo: self.repo.clone(),
                        pr_number: pr.number,
                    };
                    self.session_cache.put_pr_data(
                        cache_key,
                        PrData {
                            pr_updated_at: pr.updated_at.clone(),
                            pr,
                            files,
                        },
                    );
                }
                // Note: stale な結果でも receiver は維持する（永続リトライループ対応）
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.data_receiver = None;
            }
        }
    }

    /// コメント取得のポーリング
    pub(crate) fn poll_comment_updates(&mut self) {
        let Some((origin_pr, rx)) = self.comment_receiver.as_mut() else {
            return;
        };
        let origin_pr = *origin_pr;

        match rx.try_recv() {
            Ok(Ok(comments)) => {
                // セッションキャッシュに格納（発信元PRのキーで保存）
                let cache_key = PrCacheKey {
                    repo: self.repo.clone(),
                    pr_number: origin_pr,
                };
                self.session_cache
                    .put_review_comments(cache_key, comments.clone());
                // PR が切り替わっていなければ UI 状態にも反映
                if self.pr_number == Some(origin_pr) {
                    self.review_comments = Some(comments);
                    self.selected_comment = 0;
                    self.comment_list_scroll_offset = 0;
                    self.comments_loading = false;
                    // Update comment positions if in diff view or side-by-side
                    if matches!(
                        self.state,
                        AppState::DiffView | AppState::SplitViewDiff | AppState::SplitViewFileList
                    ) {
                        self.update_file_comment_positions();
                        self.ensure_diff_cache();
                    }
                }
                self.comment_receiver = None;
            }
            Ok(Err(e)) => {
                eprintln!("Warning: Failed to fetch comments: {}", e);
                // Keep existing comments if any, or show empty
                if self.pr_number == Some(origin_pr) {
                    if self.review_comments.is_none() {
                        self.review_comments = Some(vec![]);
                    }
                    self.comments_loading = false;
                }
                self.comment_receiver = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // Keep existing comments if any, or show empty
                if self.pr_number == Some(origin_pr) {
                    if self.review_comments.is_none() {
                        self.review_comments = Some(vec![]);
                    }
                    self.comments_loading = false;
                }
                self.comment_receiver = None;
            }
        }
    }

    /// バックグラウンドdiffキャッシュ構築のポーリング
    pub(crate) fn poll_diff_cache_updates(&mut self) {
        let Some(ref mut rx) = self.diff_cache_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(cache) => {
                // DataState::Loaded でなければ破棄（PR遷移中のstaleキャッシュ防止）
                if !matches!(self.data_state, DataState::Loaded { .. }) {
                    self.diff_cache_receiver = None;
                    return;
                }
                // バリデーション: ファイル切替されていないか確認
                if cache.file_index != self.selected_file {
                    self.diff_cache_receiver = None;
                    return;
                }
                // patch変更されていないか確認（ファイルが存在しない場合も破棄）
                let Some(file) = self.files().get(self.selected_file) else {
                    self.diff_cache_receiver = None;
                    return;
                };
                let Some(ref patch) = file.patch else {
                    self.diff_cache_receiver = None;
                    return;
                };
                if cache.patch_hash != hash_string(patch) {
                    self.diff_cache_receiver = None;
                    return;
                }
                // キャッシュをスワップ（再描画は次フレームで自動）
                self.diff_cache = Some(cache);
                self.diff_cache_receiver = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.diff_cache_receiver = None;
            }
        }
    }

    /// ファイルのハイライトキャッシュを事前構築（バックグラウンド）
    ///
    /// データロード完了時に呼び出す。MAX_PREFETCH_FILES 件まで処理し、
    /// 既にキャッシュ済みのファイルはスキップする。
    pub(crate) fn start_prefetch_all_files(&mut self) {
        // 既存のプリフェッチを中断
        self.prefetch_receiver = None;

        // キャッシュ済みファイルをスキップし、上限まで収集
        // poll_prefetch_updates() で現在表示中のハイライト済みファイルはストアに格納されないため、
        // ここでも同じ条件で除外する（除外しないとプリフェッチが永久ループする）
        let files: Vec<_> = self
            .files()
            .iter()
            .enumerate()
            .filter(|(i, f)| {
                f.patch.is_some()
                    && !self.highlighted_cache_store.contains_key(i)
                    && !self
                        .diff_cache
                        .as_ref()
                        .is_some_and(|c| c.file_index == *i && c.highlighted)
            })
            .take(MAX_PREFETCH_FILES)
            .map(|(i, f)| (i, f.filename.clone(), f.patch.clone().unwrap()))
            .collect();

        if files.is_empty() {
            return;
        }

        let theme = self.config.diff.theme.clone();
        let markdown_rich = self.markdown_rich;
        let tab_width = self.config.diff.tab_width;
        let channel_size = files.len().min(MAX_PREFETCH_FILES);
        let (tx, rx) = mpsc::channel(channel_size);
        self.prefetch_receiver = Some(rx);

        tokio::task::spawn_blocking(move || {
            let mut parser_pool = ParserPool::new();

            for (index, filename, patch) in &files {
                let mut cache = crate::ui::diff_view::build_diff_cache(
                    patch,
                    filename,
                    &theme,
                    &mut parser_pool,
                    markdown_rich,
                    tab_width,
                );
                cache.file_index = *index;
                if tx.blocking_send(cache).is_err() {
                    break; // receiver がドロップされた
                }
            }
        });
    }

    /// プリフェッチ結果をポーリングして highlighted_cache_store に格納
    pub(crate) fn poll_prefetch_updates(&mut self) {
        let Some(ref mut rx) = self.prefetch_receiver else {
            return;
        };

        loop {
            match rx.try_recv() {
                Ok(cache) => {
                    let file_index = cache.file_index;
                    // 現在表示中でハイライト済みならスキップ
                    if self
                        .diff_cache
                        .as_ref()
                        .is_some_and(|c| c.file_index == file_index && c.highlighted)
                    {
                        continue;
                    }
                    // ストアに既にあればスキップ
                    if self.highlighted_cache_store.contains_key(&file_index) {
                        continue;
                    }
                    // サイズ上限チェック: 超過時は現在選択中のファイルから最も遠いエントリを削除
                    if self.highlighted_cache_store.len() >= MAX_HIGHLIGHTED_CACHE_ENTRIES {
                        // 現在選択中のファイルから最も遠いエントリを削除
                        let evict_key = self
                            .highlighted_cache_store
                            .keys()
                            .max_by_key(|k| (**k).abs_diff(self.selected_file))
                            .copied();
                        if let Some(key) = evict_key {
                            self.highlighted_cache_store.remove(&key);
                        }
                    }
                    self.highlighted_cache_store.insert(file_index, cache);
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.prefetch_receiver = None;
                    // プリフェッチ完了後、まだ未キャッシュのファイルがあれば再起動
                    // （バッチ進行中に新しい patch が到着した分をカバーする）
                    self.start_prefetch_all_files();
                    break;
                }
            }
        }
    }

    // ========================================
    // 2段階ロード: バッチ diff / オンデマンド diff
    // ========================================

    /// Phase 2: データロード後、BG バッチ diff ロードを開始（local mode 専用）
    pub(crate) fn start_batch_diff_loading(&mut self) {
        let mut tracked_filenames: Vec<String> = Vec::new();
        let mut untracked_filenames: Vec<String> = Vec::new();

        for f in self.files() {
            if f.patch.is_some() {
                continue;
            }
            if f.status == "added" {
                // added かつ numstat が 0/0 → untracked の可能性が高い
                // name-status に A として出てくるファイルは tracked（git add 済み）
                // untracked はリスト順で merge_untracked_files_lazy で追加される
                // status == "added" && additions == 0 && deletions == 0 → untracked
                if f.additions == 0 && f.deletions == 0 {
                    untracked_filenames.push(f.filename.clone());
                } else {
                    tracked_filenames.push(f.filename.clone());
                }
            } else {
                tracked_filenames.push(f.filename.clone());
            }
        }

        if tracked_filenames.is_empty() && untracked_filenames.is_empty() {
            // 全ファイルが既に patch を持っている → プリフェッチ開始
            self.start_prefetch_all_files();
            return;
        }

        let total_batches = (tracked_filenames.len() + untracked_filenames.len()).div_ceil(20) + 1;
        let (tx, rx) = mpsc::channel(total_batches);
        self.batch_diff_receiver = Some(rx);

        let working_dir = self.working_dir.clone();
        tokio::spawn(async move {
            crate::loader::fetch_local_diffs_batched(
                working_dir,
                tracked_filenames,
                untracked_filenames,
                20,
                tx,
            )
            .await;
        });
    }

    /// BG バッチ diff の結果をポーリングして files に適用
    ///
    /// poll_prefetch_updates() と同様にループで全バッチを一括ドレインする。
    /// 1 tick に1バッチだと 340バッチ × 100ms = 34秒かかるため、
    /// 利用可能な全バッチをまとめて処理する。
    pub(crate) fn poll_batch_diff_updates(&mut self) {
        let Some(ref mut rx) = self.batch_diff_receiver else {
            return;
        };

        let mut current_file_updated = false;
        let mut any_received = false;

        // インデックスマップをループ外で1回だけ構築
        let index_map: Option<HashMap<String, usize>> =
            if let DataState::Loaded { ref files, .. } = self.data_state {
                Some(
                    files
                        .iter()
                        .enumerate()
                        .map(|(i, f)| (f.filename.clone(), i))
                        .collect(),
                )
            } else {
                None
            };

        loop {
            match rx.try_recv() {
                Ok(results) => {
                    any_received = true;

                    if let DataState::Loaded { ref mut files, .. } = self.data_state {
                        if let Some(ref index_map) = index_map {
                            for result in &results {
                                if let Some(&idx) = index_map.get(&result.filename) {
                                    if files[idx].patch.is_none() {
                                        files[idx].patch = result.patch.clone();
                                        if idx == self.selected_file {
                                            current_file_updated = true;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // SessionCache も更新
                    if let Some(pr_number) = self.pr_number {
                        let key = PrCacheKey {
                            repo: self.repo.clone(),
                            pr_number,
                        };
                        for result in &results {
                            self.session_cache.update_file_patch(
                                &key,
                                &result.filename,
                                result.patch.clone(),
                            );
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.batch_diff_receiver = None;
                    // 全バッチ完了: patch 含むシグネチャを更新し、コンテンツ変更を検出
                    if self.local_mode {
                        self.update_patch_signatures_and_auto_focus();
                    }
                    break;
                }
            }
        }

        // ループ終了後にまとめて後処理
        if current_file_updated {
            self.diff_cache = None;
            self.diff_cache_receiver = None;
            self.lazy_diff_receiver = None;
            self.lazy_diff_pending_file = None;
            self.update_diff_line_count();
            self.ensure_diff_cache();
        }

        if any_received && self.prefetch_receiver.is_none() {
            self.start_prefetch_all_files();
        }
    }

    /// 選択中ファイルの patch が None なら BG で単一 diff を即時取得
    pub(crate) fn request_lazy_diff(&mut self) {
        if !self.local_mode {
            return;
        }
        let file = self.files().get(self.selected_file);
        let Some(file) = file else { return };
        if file.patch.is_some() {
            return;
        }

        let filename = file.filename.clone();
        if self.lazy_diff_pending_file.as_deref() == Some(&filename) {
            return;
        }

        // untracked 判定: status == "added" && additions == 0 && deletions == 0
        let is_untracked = file.status == "added" && file.additions == 0 && file.deletions == 0;

        let (tx, rx) = mpsc::channel(1);
        self.lazy_diff_receiver = Some(rx);
        self.lazy_diff_pending_file = Some(filename.clone());

        let working_dir = self.working_dir.clone();
        tokio::spawn(async move {
            crate::loader::fetch_single_file_diff(working_dir, filename, is_untracked, tx).await;
        });
    }

    /// lazy diff の結果をポーリングして適用
    pub(crate) fn poll_lazy_diff_updates(&mut self) {
        let Some(ref mut rx) = self.lazy_diff_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => {
                self.lazy_diff_receiver = None;
                self.lazy_diff_pending_file = None;

                if let DataState::Loaded { ref mut files, .. } = self.data_state {
                    if let Some(file) = files.iter_mut().find(|f| f.filename == result.filename) {
                        // バッチが先に到着済みなら上書きしない（重複適用防止）
                        if file.patch.is_none() {
                            file.patch = result.patch.clone();
                        }
                    }
                }

                // SessionCache も更新
                if let Some(pr_number) = self.pr_number {
                    let key = PrCacheKey {
                        repo: self.repo.clone(),
                        pr_number,
                    };
                    self.session_cache
                        .update_file_patch(&key, &result.filename, result.patch);
                }

                self.diff_cache = None;
                self.diff_cache_receiver = None;
                self.update_diff_line_count();
                self.ensure_diff_cache();
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.lazy_diff_receiver = None;
                self.lazy_diff_pending_file = None;
            }
        }
    }

    /// UI 用: lazy diff がロード中かどうか
    pub fn is_lazy_diff_loading(&self) -> bool {
        self.lazy_diff_pending_file.is_some()
            || (self.local_mode
                && self
                    .files()
                    .get(self.selected_file)
                    .is_some_and(|f| f.patch.is_none())
                && self.batch_diff_receiver.is_some())
    }

    /// Discussion コメント取得のポーリング
    pub(crate) fn poll_discussion_comment_updates(&mut self) {
        let Some((origin_pr, rx)) = self.discussion_comment_receiver.as_mut() else {
            return;
        };
        let origin_pr = *origin_pr;

        match rx.try_recv() {
            Ok(Ok(comments)) => {
                // セッションキャッシュに格納（発信元PRのキーで保存）
                let cache_key = PrCacheKey {
                    repo: self.repo.clone(),
                    pr_number: origin_pr,
                };
                self.session_cache
                    .put_discussion_comments(cache_key, comments.clone());
                // PR が切り替わっていなければ UI 状態にも反映
                if self.pr_number == Some(origin_pr) {
                    self.discussion_comments = Some(comments);
                    self.selected_discussion_comment = 0;
                    self.discussion_comments_loading = false;
                }
                self.discussion_comment_receiver = None;
            }
            Ok(Err(e)) => {
                eprintln!("Warning: Failed to fetch discussion comments: {}", e);
                if self.pr_number == Some(origin_pr) {
                    if self.discussion_comments.is_none() {
                        self.discussion_comments = Some(vec![]);
                    }
                    self.discussion_comments_loading = false;
                }
                self.discussion_comment_receiver = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                if self.pr_number == Some(origin_pr) {
                    if self.discussion_comments.is_none() {
                        self.discussion_comments = Some(vec![]);
                    }
                    self.discussion_comments_loading = false;
                }
                self.discussion_comment_receiver = None;
            }
        }
    }

    /// コメント送信結果のポーリング
    pub(crate) fn poll_comment_submit_updates(&mut self) {
        // Clear old submission result after 3 seconds
        if let Some(time) = self.submission_result_time {
            if time.elapsed().as_secs() >= 3 {
                self.submission_result = None;
                self.submission_result_time = None;
            }
        }

        let Some((origin_pr, rx)) = self.comment_submit_receiver.as_mut() else {
            return;
        };
        let origin_pr = *origin_pr;

        match rx.try_recv() {
            Ok(CommentSubmitResult::Success) => {
                self.comment_submitting = false;
                self.comment_submit_receiver = None;
                self.submission_result = Some((true, "Submitted".to_string()));
                self.submission_result_time = Some(Instant::now());
                // インメモリキャッシュを破棄してコメントを再取得
                let cache_key = PrCacheKey {
                    repo: self.repo.clone(),
                    pr_number: origin_pr,
                };
                self.session_cache.remove_review_comments(&cache_key);
                // PR が切り替わっていなければコメントを再取得
                if self.pr_number == Some(origin_pr) {
                    self.review_comments = None;
                    self.load_review_comments();
                    self.update_file_comment_positions();
                }
            }
            Ok(CommentSubmitResult::Error(e)) => {
                self.comment_submitting = false;
                self.comment_submit_receiver = None;
                self.submission_result = Some((false, format!("Failed: {}", e)));
                self.submission_result_time = Some(Instant::now());
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.comment_submitting = false;
                self.comment_submit_receiver = None;
            }
        }
    }

    pub(crate) fn poll_mark_viewed_updates(&mut self) {
        let Some((origin_pr, ref mut rx)) = self.mark_viewed_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(MarkViewedResult::Completed {
                marked_paths,
                total_targets,
                error,
                set_viewed,
            }) => {
                self.mark_viewed_receiver = None;

                if self.pr_number == Some(origin_pr) {
                    self.apply_viewed_state_to_files(&marked_paths, set_viewed);
                }

                let action_label = if set_viewed { "viewed" } else { "unviewed" };
                match error {
                    Some(err) => {
                        if marked_paths.is_empty() {
                            self.submission_result =
                                Some((false, format!("Mark {} failed: {}", action_label, err)));
                        } else {
                            self.submission_result = Some((
                                false,
                                format!(
                                    "Marked {}/{} files as {}, then failed: {}",
                                    marked_paths.len(),
                                    total_targets,
                                    action_label,
                                    err
                                ),
                            ));
                        }
                    }
                    None => {
                        self.submission_result = Some((
                            true,
                            format!("Marked {} file(s) as {}", marked_paths.len(), action_label),
                        ));
                    }
                }
                self.submission_result_time = Some(Instant::now());
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.mark_viewed_receiver = None;
            }
        }
    }

    pub(crate) fn apply_viewed_state_to_files(&mut self, marked_paths: &[String], set_viewed: bool) {
        if marked_paths.is_empty() {
            return;
        }

        let marked_set: HashSet<&str> = marked_paths.iter().map(|path| path.as_str()).collect();
        if let DataState::Loaded { files, .. } = &mut self.data_state {
            for file in files.iter_mut() {
                if marked_set.contains(file.filename.as_str()) {
                    file.viewed = set_viewed;
                }
            }
        }

        self.sync_loaded_data_to_cache();
    }

    pub(crate) fn sync_loaded_data_to_cache(&mut self) {
        let DataState::Loaded { pr, files } = &self.data_state else {
            return;
        };
        let Some(pr_number) = self.pr_number else {
            return;
        };

        let cache_key = PrCacheKey {
            repo: self.repo.clone(),
            pr_number,
        };
        self.session_cache.put_pr_data(
            cache_key,
            PrData {
                pr: pr.clone(),
                files: files.clone(),
                pr_updated_at: pr.updated_at.clone(),
            },
        );
    }

    pub(crate) fn poll_rally_events(&mut self) {
        let Some(ref mut rx) = self.rally_event_receiver else {
            return;
        };

        // Process all available events
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    if let Some(ref mut rally_state) = self.ai_rally_state {
                        match &event {
                            RallyEvent::StateChanged(state) => {
                                rally_state.state = *state;
                                // Clear pending post info on terminal states
                                if matches!(
                                    state,
                                    RallyState::Completed | RallyState::Aborted | RallyState::Error
                                ) {
                                    rally_state.pending_review_post = None;
                                    rally_state.pending_fix_post = None;
                                }
                            }
                            RallyEvent::IterationStarted(i) => {
                                rally_state.iteration = *i;
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Info,
                                    format!("Starting iteration {}", i),
                                ));
                            }
                            RallyEvent::Log(msg) => {
                                rally_state
                                    .push_log(LogEntry::new(LogEventType::Info, msg.clone()));
                            }
                            RallyEvent::AgentThinking(content) => {
                                // Store full content; truncation happens at display time
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Thinking,
                                    content.clone(),
                                ));
                            }
                            RallyEvent::AgentToolUse(tool_name, input) => {
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::ToolUse,
                                    format!("{}: {}", tool_name, input),
                                ));
                            }
                            RallyEvent::AgentToolResult(tool_name, result) => {
                                // Store full content; truncation happens at display time
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::ToolResult,
                                    format!("{}: {}", tool_name, result),
                                ));
                            }
                            RallyEvent::AgentText(text) => {
                                // Store full content; truncation happens at display time
                                rally_state
                                    .push_log(LogEntry::new(LogEventType::Text, text.clone()));
                            }
                            RallyEvent::ReviewCompleted(_) => {
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Review,
                                    "Review completed".to_string(),
                                ));
                            }
                            RallyEvent::FixCompleted(fix) => {
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Fix,
                                    format!("Fix completed: {}", fix.summary),
                                ));
                            }
                            RallyEvent::Error(e) => {
                                rally_state.push_log(LogEntry::new(LogEventType::Error, e.clone()));
                            }
                            RallyEvent::ClarificationNeeded(question) => {
                                rally_state.pending_question = Some(question.clone());
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Info,
                                    format!("Clarification needed: {}", question),
                                ));
                            }
                            RallyEvent::PermissionNeeded(action, reason) => {
                                rally_state.pending_permission = Some(PermissionInfo {
                                    action: action.clone(),
                                    reason: reason.clone(),
                                });
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Info,
                                    format!("Permission needed: {} - {}", action, reason),
                                ));
                            }
                            RallyEvent::ReviewPostConfirmNeeded(info) => {
                                rally_state.pending_review_post = Some(info.clone());
                                rally_state.pending_fix_post = None; // exclusive
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Info,
                                    format!(
                                        "Review post confirmation needed: {} ({} comments)",
                                        info.action, info.comment_count
                                    ),
                                ));
                            }
                            RallyEvent::FixPostConfirmNeeded(info) => {
                                rally_state.pending_fix_post = Some(info.clone());
                                rally_state.pending_review_post = None; // exclusive
                                rally_state.push_log(LogEntry::new(
                                    LogEventType::Info,
                                    format!(
                                        "Fix post confirmation needed: {} file(s) modified",
                                        info.files_modified.len()
                                    ),
                                ));
                            }
                            _ => {}
                        }
                        rally_state.history.push(event);
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    self.rally_event_receiver = None;
                    if let Some(ref mut rally_state) = self.ai_rally_state {
                        if rally_state.state.is_active() {
                            rally_state.state = RallyState::Error;
                            rally_state.push_log(LogEntry::new(
                                LogEventType::Error,
                                "Rally process terminated unexpectedly".to_string(),
                            ));
                        }
                    }
                    break;
                }
            }
        }
    }
    pub(crate) fn handle_data_result(&mut self, origin_pr: u32, result: DataLoadResult) {
        // [Critical] watcher refresh 時に古いバッチ/lazy diff 結果が誤適用されるのを防止
        self.batch_diff_receiver = None;
        self.lazy_diff_receiver = None;
        self.lazy_diff_pending_file = None;

        match result {
            DataLoadResult::Success { pr, files } => {
                let changed_file_index = if self.local_mode && self.local_auto_focus {
                    self.find_changed_local_file_index(&files, self.selected_file)
                } else {
                    None
                };
                let old_selected_file = self
                    .files()
                    .get(self.selected_file)
                    .map(|file| file.filename.clone());
                let old_selected = self.selected_file;
                let mut next_selected = if files.is_empty() {
                    0
                } else if let Some(filename) = old_selected_file {
                    files
                        .iter()
                        .position(|file| file.filename == filename)
                        .unwrap_or_else(|| self.selected_file.min(files.len() - 1))
                } else {
                    self.selected_file.min(files.len() - 1)
                };

                if let Some(idx) = changed_file_index {
                    next_selected = idx;
                }

                if next_selected != old_selected {
                    self.diff_cache = None;
                    self.diff_cache_receiver = None;
                    self.selected_line = 0;
                    self.scroll_offset = 0;
                    self.comment_panel_open = false;
                    self.comment_panel_scroll = 0;
                }

                self.selected_file = next_selected;
                if changed_file_index.is_some() {
                    self.file_list_scroll_offset =
                        self.file_list_scroll_offset.min(self.selected_file);

                    // BG rally 中は state 遷移をスキップ（ファイル選択のみ更新）
                    let rally_running_in_bg = self
                        .ai_rally_state
                        .as_ref()
                        .map(|s| s.state.is_active())
                        .unwrap_or(false)
                        && !matches!(self.state, AppState::AiRally);

                    if !rally_running_in_bg
                        && matches!(self.state, AppState::FileList | AppState::SplitViewFileList)
                    {
                        self.state = AppState::SplitViewDiff;
                    }
                    self.sync_diff_to_selected_file();
                } else {
                    self.file_list_scroll_offset =
                        self.file_list_scroll_offset.min(self.selected_file);
                }
                self.diff_line_count = Self::calc_diff_line_count(&files, self.selected_file);
                // ファイル一覧が変わるため、ハイライトキャッシュストアをクリア
                self.highlighted_cache_store.clear();
                // Check if we need to start AI Rally (--ai-rally flag was passed)
                let should_start_rally =
                    self.start_ai_rally_on_load && matches!(self.data_state, DataState::Loading);
                // clone() でキャッシュと DataState の両方にデータを格納（Arc不使用）
                let cache_key = PrCacheKey {
                    repo: self.repo.clone(),
                    pr_number: origin_pr,
                };
                let local_files_for_signature = if self.local_mode {
                    Some(files.clone())
                } else {
                    None
                };
                self.session_cache.put_pr_data(
                    cache_key,
                    PrData {
                        pr: pr.clone(),
                        files: files.clone(),
                        pr_updated_at: pr.updated_at.clone(),
                    },
                );
                self.data_state = DataState::Loaded { pr, files };
                // ファイル一覧が変わったため、フィルタを再適用（stale indices 防止）
                if self.file_list_filter.is_some() {
                    self.reapply_filter("file");
                }
                // selected_file が変更された場合、コメント位置キャッシュを再計算
                if self.selected_file != old_selected {
                    self.update_file_comment_positions();
                }
                // Local mode: trigger lazy diff for the selected file even when
                // auto-focus didn't move selection, so the user doesn't have to
                // wait for the full batch order. Must be after DataState::Loaded
                // so that self.files() returns the new file list.
                if self.local_mode {
                    self.request_lazy_diff();
                }
                // local mode: バッチ diff ロード → 完了後にプリフェッチ開始
                // PR mode: 即座にプリフェッチ開始
                if self.local_mode {
                    self.start_batch_diff_loading();
                } else {
                    self.start_prefetch_all_files();
                }
                if should_start_rally {
                    self.start_ai_rally_on_load = false; // Clear the flag
                    self.start_ai_rally();
                }
                if let Some(local_files) = local_files_for_signature {
                    self.remember_local_file_signatures(&local_files);
                }
                // Local モードのデータ処理完了後、ウォッチャーの debounce フラグをリセット。
                // app.rs の activate_watcher で作成した refresh_pending は main.rs の
                // リトライループとは別の Arc であるため、ここで明示的にリセットしないと
                // 最初のファイル変更イベント以降 watcher がサイレントになる。
                if self.local_mode {
                    if let Some(ref pending) = self.refresh_pending {
                        pending.store(false, Ordering::Release);
                    }
                }
                // ファイル選択変更後も差分キャッシュを即座に復旧して
                // split view 側の「Loading diff...」が発生しないようにする
                self.ensure_diff_cache();
            }
            DataLoadResult::Error(msg) => {
                // Loading状態の場合のみエラー表示（既にデータがある場合は無視）
                if matches!(self.data_state, DataState::Loading) {
                    self.data_state = DataState::Error(msg);
                }
            }
        }
    }

    /// base シグネチャ（patch 除外）: Phase 1 での構造変更検出用
    pub(crate) fn local_file_signature(file: &ChangedFile) -> u64 {
        let signature = format!(
            "{}|{}|{}|{}",
            file.filename, file.status, file.additions, file.deletions
        );
        hash_string(&signature)
    }

    /// full シグネチャ（patch 含む）: Phase 2 でのコンテンツ変更検出用
    pub(crate) fn local_file_full_signature(file: &ChangedFile) -> u64 {
        let patch = file.patch.as_deref().unwrap_or_default();
        let signature = format!(
            "{}|{}|{}|{}|{}",
            file.filename, file.status, file.additions, file.deletions, patch
        );
        hash_string(&signature)
    }

    pub(crate) fn find_changed_local_file_index(
        &self,
        files: &[ChangedFile],
        anchor_selected: usize,
    ) -> Option<usize> {
        if self.local_file_signatures.is_empty() {
            // First local snapshot loaded: auto-focus the first file on first change.
            // This is useful when starting with a clean working tree and adding files.
            return (!files.is_empty()).then_some(0);
        }

        if files.is_empty() {
            return None;
        }

        let anchor_selected = anchor_selected.min(files.len() - 1);
        let changed_indices: Vec<usize> = files
            .iter()
            .enumerate()
            .filter_map(|(idx, file)| {
                let next_signature = Self::local_file_signature(file);
                match self.local_file_signatures.get(&file.filename) {
                    Some(signature) if *signature == next_signature => None,
                    _ => Some(idx),
                }
            })
            .collect();

        if changed_indices.is_empty() {
            return None;
        }

        if changed_indices.contains(&anchor_selected) {
            return Some(anchor_selected);
        }

        if changed_indices.len() == 1 {
            return changed_indices.into_iter().next();
        }

        let next = changed_indices
            .iter()
            .copied()
            .find(|idx| *idx > anchor_selected);
        let prev = changed_indices
            .iter()
            .rev()
            .copied()
            .find(|idx| *idx < anchor_selected);

        match (next, prev) {
            (Some(next_idx), _) => Some(next_idx),
            (None, Some(prev_idx)) => Some(prev_idx),
            _ => None,
        }
    }

    pub(crate) fn remember_local_file_signatures(&mut self, files: &[ChangedFile]) {
        self.local_file_signatures.clear();
        for file in files {
            self.local_file_signatures
                .insert(file.filename.clone(), Self::local_file_signature(file));
        }
    }

    /// バッチ diff 完了後に patch 含む完全シグネチャを更新し、
    /// コンテンツ変更を検出した場合はオートフォーカスする
    pub(crate) fn update_patch_signatures_and_auto_focus(&mut self) {
        let files = match &self.data_state {
            DataState::Loaded { files, .. } => files,
            _ => return,
        };

        // 初回バッチ完了時（前回の patch シグネチャが空）はシグネチャ保存のみ
        let is_first_batch = self.local_file_patch_signatures.is_empty();

        let mut changed_index: Option<usize> = None;
        if !is_first_batch && self.local_auto_focus {
            for (idx, file) in files.iter().enumerate() {
                // patch がロードされたファイルのみ比較
                if file.patch.is_none() {
                    continue;
                }
                let new_sig = Self::local_file_full_signature(file);
                match self.local_file_patch_signatures.get(&file.filename) {
                    Some(old_sig) if *old_sig == new_sig => {}
                    Some(_) => {
                        // 内容が変わったファイルを発見
                        changed_index = Some(idx);
                        break;
                    }
                    None => {}
                }
            }
        }

        // patch シグネチャを更新
        self.local_file_patch_signatures.clear();
        for file in files {
            if file.patch.is_some() {
                self.local_file_patch_signatures
                    .insert(file.filename.clone(), Self::local_file_full_signature(file));
            }
        }

        // 変更検出時にオートフォーカス
        if let Some(idx) = changed_index {
            if idx != self.selected_file {
                self.selected_file = idx;
                self.file_list_scroll_offset = self.file_list_scroll_offset.min(idx);
                self.diff_cache = None;
                self.diff_cache_receiver = None;
                self.selected_line = 0;
                self.scroll_offset = 0;
                self.comment_panel_open = false;
                self.comment_panel_scroll = 0;
                if matches!(self.state, AppState::FileList | AppState::SplitViewFileList) {
                    self.state = AppState::SplitViewDiff;
                }
                self.sync_diff_to_selected_file();
            }
        }
    }

}
