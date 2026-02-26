/// リストフィルタ機能
///
/// PR一覧やファイル一覧でキーワードによる絞り込みを提供する。
/// `matched_indices` は元リストへのインデックスを保持し、
/// 既存のナビゲーションロジック（`selected_file`, `selected_pr`）との整合性を維持する。

/// リストフィルタの状態
pub struct ListFilter {
    /// フィルタ文字列
    pub query: String,
    /// カーソル位置（char単位、Unicode安全）
    pub cursor_chars: usize,
    /// 元リストへのインデックス（マッチした項目のみ）
    pub matched_indices: Vec<usize>,
    /// matched_indices 内の選択位置（0件時は None）
    pub selected: Option<usize>,
    /// 入力バー表示中か
    pub input_active: bool,
}

impl ListFilter {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor_chars: 0,
            matched_indices: Vec::new(),
            selected: None,
            input_active: true,
        }
    }

    /// クロージャベースの汎用マッチングでフィルタを適用する。
    ///
    /// `matches` クロージャは各アイテムとクエリ（小文字化済み）を受け取り、
    /// マッチするかどうかを返す。
    pub fn apply<T>(&mut self, items: &[T], matches: impl Fn(&T, &str) -> bool) {
        let query_lower = self.query.to_lowercase();
        self.matched_indices = if query_lower.is_empty() {
            (0..items.len()).collect()
        } else {
            items
                .iter()
                .enumerate()
                .filter(|(_, item)| matches(item, &query_lower))
                .map(|(i, _)| i)
                .collect()
        };
    }

    /// matched_indices 再計算後に selected を安全に同期する。
    ///
    /// 元リストのインデックスを返す（呼び出し側で selected_file/selected_pr に設定）。
    pub fn sync_selection(&mut self) -> Option<usize> {
        self.selected = if self.matched_indices.is_empty() {
            None
        } else {
            Some(
                self.selected
                    .unwrap_or(0)
                    .min(self.matched_indices.len() - 1),
            )
        };
        self.selected.map(|s| self.matched_indices[s])
    }

    /// カーソル位置に文字を挿入する
    pub fn insert_char(&mut self, c: char) {
        let byte_pos = self
            .query
            .char_indices()
            .nth(self.cursor_chars)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len());
        self.query.insert(byte_pos, c);
        self.cursor_chars += 1;
    }

    /// カーソル位置の手前の文字を削除する（Backspace）
    pub fn delete_char(&mut self) {
        if self.cursor_chars == 0 {
            return;
        }
        self.cursor_chars -= 1;
        let byte_pos = self
            .query
            .char_indices()
            .nth(self.cursor_chars)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len());
        // 削除する文字のバイト長を取得
        let next_byte = self
            .query
            .char_indices()
            .nth(self.cursor_chars + 1)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len());
        self.query.replace_range(byte_pos..next_byte, "");
    }

    /// クエリを全クリアする（Ctrl+U）
    pub fn clear_query(&mut self) {
        self.query.clear();
        self.cursor_chars = 0;
    }

    /// フィルタ結果内で上に移動し、元リストのインデックスを返す
    pub fn navigate_up(&mut self) -> Option<usize> {
        if let Some(sel) = self.selected {
            if sel > 0 {
                self.selected = Some(sel - 1);
            }
        }
        self.selected.map(|s| self.matched_indices[s])
    }

    /// フィルタ結果内で下に移動し、元リストのインデックスを返す
    pub fn navigate_down(&mut self) -> Option<usize> {
        if let Some(sel) = self.selected {
            if sel + 1 < self.matched_indices.len() {
                self.selected = Some(sel + 1);
            }
        }
        self.selected.map(|s| self.matched_indices[s])
    }

    /// 現在選択中の元リストインデックスを取得する
    pub fn current_original_index(&self) -> Option<usize> {
        self.selected.map(|s| self.matched_indices[s])
    }

    /// フィルタが有効か（クエリが空でない）
    pub fn has_query(&self) -> bool {
        !self.query.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_filter_is_input_active() {
        let filter = ListFilter::new();
        assert!(filter.input_active);
        assert!(filter.query.is_empty());
        assert_eq!(filter.cursor_chars, 0);
        assert!(filter.matched_indices.is_empty());
        assert_eq!(filter.selected, None);
    }

    #[test]
    fn test_apply_empty_query_returns_all() {
        let items = vec!["foo", "bar", "baz"];
        let mut filter = ListFilter::new();
        filter.apply(&items, |item, _q| item.contains(_q));
        assert_eq!(filter.matched_indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_apply_filters_correctly() {
        let items = vec!["hello", "world", "help", "test"];
        let mut filter = ListFilter::new();
        filter.query = "hel".to_string();
        filter.apply(&items, |item, q| item.to_lowercase().contains(q));
        assert_eq!(filter.matched_indices, vec![0, 2]);
    }

    #[test]
    fn test_apply_case_insensitive() {
        let items = vec!["Hello", "WORLD", "help"];
        let mut filter = ListFilter::new();
        filter.query = "HEL".to_string();
        filter.apply(&items, |item, q| item.to_lowercase().contains(q));
        assert_eq!(filter.matched_indices, vec![0, 2]);
    }

    #[test]
    fn test_apply_no_matches() {
        let items = vec!["foo", "bar"];
        let mut filter = ListFilter::new();
        filter.query = "xyz".to_string();
        filter.apply(&items, |item, q| item.to_lowercase().contains(q));
        assert!(filter.matched_indices.is_empty());
    }

    #[test]
    fn test_sync_selection_with_matches() {
        let mut filter = ListFilter::new();
        filter.matched_indices = vec![2, 5, 8];
        filter.selected = None;

        let idx = filter.sync_selection();
        assert_eq!(filter.selected, Some(0));
        assert_eq!(idx, Some(2));
    }

    #[test]
    fn test_sync_selection_clamps() {
        let mut filter = ListFilter::new();
        filter.matched_indices = vec![1, 3];
        filter.selected = Some(10); // out of range

        let idx = filter.sync_selection();
        assert_eq!(filter.selected, Some(1)); // clamped to len-1
        assert_eq!(idx, Some(3));
    }

    #[test]
    fn test_sync_selection_empty() {
        let mut filter = ListFilter::new();
        filter.matched_indices = vec![];
        filter.selected = Some(0);

        let idx = filter.sync_selection();
        assert_eq!(filter.selected, None);
        assert_eq!(idx, None);
    }

    #[test]
    fn test_insert_char() {
        let mut filter = ListFilter::new();
        filter.insert_char('a');
        filter.insert_char('b');
        assert_eq!(filter.query, "ab");
        assert_eq!(filter.cursor_chars, 2);
    }

    #[test]
    fn test_insert_char_unicode() {
        let mut filter = ListFilter::new();
        filter.insert_char('日');
        filter.insert_char('本');
        assert_eq!(filter.query, "日本");
        assert_eq!(filter.cursor_chars, 2);
    }

    #[test]
    fn test_delete_char() {
        let mut filter = ListFilter::new();
        filter.query = "abc".to_string();
        filter.cursor_chars = 3;
        filter.delete_char();
        assert_eq!(filter.query, "ab");
        assert_eq!(filter.cursor_chars, 2);
    }

    #[test]
    fn test_delete_char_at_start() {
        let mut filter = ListFilter::new();
        filter.query = "abc".to_string();
        filter.cursor_chars = 0;
        filter.delete_char();
        assert_eq!(filter.query, "abc"); // no change
        assert_eq!(filter.cursor_chars, 0);
    }

    #[test]
    fn test_delete_char_unicode() {
        let mut filter = ListFilter::new();
        filter.query = "日本語".to_string();
        filter.cursor_chars = 3;
        filter.delete_char();
        assert_eq!(filter.query, "日本");
        assert_eq!(filter.cursor_chars, 2);
    }

    #[test]
    fn test_clear_query() {
        let mut filter = ListFilter::new();
        filter.query = "hello".to_string();
        filter.cursor_chars = 3;
        filter.clear_query();
        assert!(filter.query.is_empty());
        assert_eq!(filter.cursor_chars, 0);
    }

    #[test]
    fn test_navigate_down() {
        let mut filter = ListFilter::new();
        filter.matched_indices = vec![0, 2, 4];
        filter.selected = Some(0);

        assert_eq!(filter.navigate_down(), Some(2));
        assert_eq!(filter.selected, Some(1));
        assert_eq!(filter.navigate_down(), Some(4));
        assert_eq!(filter.selected, Some(2));
        // at end, stay
        assert_eq!(filter.navigate_down(), Some(4));
        assert_eq!(filter.selected, Some(2));
    }

    #[test]
    fn test_navigate_up() {
        let mut filter = ListFilter::new();
        filter.matched_indices = vec![0, 2, 4];
        filter.selected = Some(2);

        assert_eq!(filter.navigate_up(), Some(2));
        assert_eq!(filter.selected, Some(1));
        assert_eq!(filter.navigate_up(), Some(0));
        assert_eq!(filter.selected, Some(0));
        // at start, stay
        assert_eq!(filter.navigate_up(), Some(0));
        assert_eq!(filter.selected, Some(0));
    }

    #[test]
    fn test_navigate_with_no_selection() {
        let mut filter = ListFilter::new();
        filter.matched_indices = vec![];
        filter.selected = None;

        assert_eq!(filter.navigate_down(), None);
        assert_eq!(filter.navigate_up(), None);
    }

    #[test]
    fn test_has_query() {
        let mut filter = ListFilter::new();
        assert!(!filter.has_query());
        filter.query = "x".to_string();
        assert!(filter.has_query());
    }

    #[test]
    fn test_full_workflow() {
        let items = vec!["alpha", "beta", "gamma", "delta"];
        let mut filter = ListFilter::new();

        // Type "a"
        filter.insert_char('a');
        filter.apply(&items, |item, q| item.to_lowercase().contains(q));
        let idx = filter.sync_selection();
        assert_eq!(filter.matched_indices, vec![0, 1, 2, 3]); // all contain 'a'
        assert_eq!(idx, Some(0));

        // Navigate down
        let idx = filter.navigate_down();
        assert_eq!(idx, Some(1)); // beta

        // Type more: "al"
        filter.insert_char('l');
        filter.apply(&items, |item, q| item.to_lowercase().contains(q));
        let idx = filter.sync_selection();
        assert_eq!(filter.matched_indices, vec![0]); // alpha only
        assert_eq!(filter.selected, Some(0)); // clamped from 1 to 0
        assert_eq!(idx, Some(0));
    }
}
