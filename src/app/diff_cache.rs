use tokio::sync::mpsc;

use crate::github::{ChangedFile, PullRequest};
use crate::syntax::ParserPool;

use super::types::*;
use super::{App, DataState, MAX_HIGHLIGHTED_CACHE_ENTRIES};

impl App {
    pub(crate) fn calc_diff_line_count(files: &[ChangedFile], selected: usize) -> usize {
        files
            .get(selected)
            .and_then(|f| f.patch.as_ref())
            .map(|p| p.lines().count())
            .unwrap_or(0)
    }

    pub fn files(&self) -> &[ChangedFile] {
        match &self.data_state {
            DataState::Loaded { files, .. } => files,
            _ => &[],
        }
    }

    pub fn pr(&self) -> Option<&PullRequest> {
        match &self.data_state {
            DataState::Loaded { pr, .. } => Some(pr.as_ref()),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn is_data_available(&self) -> bool {
        matches!(self.data_state, DataState::Loaded { .. })
    }
    pub(crate) fn update_diff_line_count(&mut self) {
        self.diff_line_count = Self::calc_diff_line_count(self.files(), self.selected_file);
    }

    /// Split Viewでファイル選択変更時にdiff状態を同期
    pub(crate) fn sync_diff_to_selected_file(&mut self) {
        self.selected_line = 0;
        self.scroll_offset = 0;
        self.multiline_selection = None;
        self.comment_panel_open = false;
        self.comment_panel_scroll = 0;
        self.clear_pending_keys();
        self.symbol_popup = None;
        self.update_diff_line_count();
        if !self.local_mode && self.review_comments.is_none() {
            self.load_review_comments();
        }
        self.update_file_comment_positions();
        self.request_lazy_diff();
        self.ensure_diff_cache();
    }
    pub fn ensure_diff_cache(&mut self) {
        let file_index = self.selected_file;
        let markdown_rich = self.markdown_rich;

        // 1. 現在の diff_cache が有効か確認（O(1)）
        if let Some(ref cache) = self.diff_cache {
            if cache.file_index == file_index && cache.markdown_rich == markdown_rich {
                let Some(file) = self.files().get(file_index) else {
                    self.diff_cache = None;
                    return;
                };
                let Some(ref patch) = file.patch else {
                    self.diff_cache = None;
                    return;
                };
                let current_hash = hash_string(patch);
                if cache.patch_hash == current_hash {
                    return; // キャッシュ有効
                }
            }
        }

        // 古い receiver をドロップ（競合防止）
        self.diff_cache_receiver = None;

        // 現在のハイライト済みキャッシュをストアに退避（上限チェック付き）
        if let Some(cache) = self.diff_cache.take() {
            if cache.highlighted
                && self.highlighted_cache_store.len() < MAX_HIGHLIGHTED_CACHE_ENTRIES
            {
                self.highlighted_cache_store.insert(cache.file_index, cache);
            }
        }

        let Some(file) = self.files().get(file_index) else {
            self.diff_cache = None;
            return;
        };
        let Some(patch) = file.patch.clone() else {
            self.diff_cache = None;
            return;
        };
        let filename = file.filename.clone();

        // 2. ストアにハイライト済みキャッシュがあるか確認
        if let Some(cached) = self.highlighted_cache_store.remove(&file_index) {
            let current_hash = hash_string(&patch);
            if cached.patch_hash == current_hash && cached.markdown_rich == markdown_rich {
                self.diff_cache = Some(cached);
                return; // ストアから復元、バックグラウンド構築不要
            }
            // 無効なキャッシュは破棄
        }

        // 3. キャッシュミス: プレーンキャッシュを即座に構築（~1ms）
        let tab_width = self.config.diff.tab_width;
        let mut plain_cache = crate::ui::diff_view::build_plain_diff_cache(&patch, tab_width);
        plain_cache.file_index = file_index;
        self.diff_cache = Some(plain_cache);

        // 完全版キャッシュをバックグラウンドで構築
        let (tx, rx) = mpsc::channel(1);
        self.diff_cache_receiver = Some(rx);

        let theme = self.config.diff.theme.clone();

        tokio::task::spawn_blocking(move || {
            let mut parser_pool = ParserPool::new();
            let mut cache = crate::ui::diff_view::build_diff_cache(
                &patch,
                &filename,
                &theme,
                &mut parser_pool,
                markdown_rich,
                tab_width,
            );
            cache.file_index = file_index;
            let _ = tx.try_send(cache);
        });
    }
}
