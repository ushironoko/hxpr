use anyhow::Result;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::ai::orchestrator::{OrchestratorCommand, RallyEvent};
use crate::cache::SessionCache;
use crate::config::Config;
use crate::filter::ListFilter;
use crate::github::comment::{DiscussionComment, ReviewComment};
use crate::github::{self, PrStateFilter, PullRequestSummary};
use crate::keybinding::KeyBinding;
use crate::loader::{CommentSubmitResult, DataLoadResult, SingleFileDiffResult};
use crate::ui;
use crate::ui::text_area::TextArea;
use std::time::Instant;

mod types;
pub use types::*;

mod polling;
mod input;
mod input_diff;
mod input_text;
mod comments;
mod diff_cache;
mod ai_rally;
mod key_sequence;
mod filter;
mod pr_list;
mod local_mode;
mod symbol;
#[cfg(test)]
mod tests;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// ハイライトキャッシュストアの最大エントリ数（メモリ上限）
///
/// 大規模PRでのOOM防止。超過時は現在選択中のファイルから最も遠いエントリを削除。
const MAX_HIGHLIGHTED_CACHE_ENTRIES: usize = 50;

/// プリフェッチ対象ファイルの最大数
///
/// 大規模PRで全ファイルをクローンしないよう制限。
const MAX_PREFETCH_FILES: usize = 50;

/// PR番号と紐づいたレシーバー（発信元PRを追跡してクロスPRキャッシュ汚染を防止）
type PrReceiver<T> = Option<(u32, mpsc::Receiver<T>)>;

pub struct App {
    pub repo: String,
    /// 選択されたPR番号（PR一覧から選択した場合は後から設定）
    pub pr_number: Option<u32>,
    pub data_state: DataState,
    pub state: AppState,
    // PR list state
    pub pr_list: Option<Vec<PullRequestSummary>>,
    pub selected_pr: usize,
    pub pr_list_scroll_offset: usize,
    pub pr_list_loading: bool,
    pub pr_list_has_more: bool,
    pub pr_list_state_filter: PrStateFilter,
    /// PR一覧から開始したかどうか（戻り先判定用）
    pub started_from_pr_list: bool,
    /// ローカル差分監視モードかどうか
    local_mode: bool,
    /// `--auto-focus` オプション（ローカル差分時）
    local_auto_focus: bool,
    /// 直近のローカルファイル署名（差分変更を検出、base: patch 除外）
    local_file_signatures: HashMap<String, u64>,
    /// patch 内容を含む完全シグネチャ（バッチ diff 完了後に更新）
    local_file_patch_signatures: HashMap<String, u64>,
    /// CLI で指定された元の PR 番号（モード復帰用）
    original_pr_number: Option<u32>,
    /// PR モードのスナップショット
    saved_pr_snapshot: Option<ViewSnapshot>,
    /// Local モードのスナップショット
    saved_local_snapshot: Option<ViewSnapshot>,
    /// ファイルウォッチャーハンドル（遅延生成）
    watcher_handle: Option<WatcherHandle>,
    /// ウォッチャー用 debounce フラグ（watcher スレッドと共有）
    refresh_pending: Option<Arc<AtomicBool>>,
    pr_list_receiver: Option<mpsc::Receiver<Result<github::PrListPage, String>>>,
    /// DiffView で q/Esc を押した時の戻り先
    pub diff_view_return_state: AppState,
    /// CommentPreview/SuggestionPreview の戻り先
    pub preview_return_state: AppState,
    /// Help/CommentList など汎用的な戻り先
    pub previous_state: AppState,
    pub selected_file: usize,
    pub file_list_scroll_offset: usize,
    pub selected_line: usize,
    pub diff_line_count: usize,
    pub scroll_offset: usize,
    /// 複数行選択モードの状態（None = 非選択モード）
    pub multiline_selection: Option<MultilineSelection>,
    /// 統一入力モード
    pub input_mode: Option<InputMode>,
    /// 統一入力テキストエリア
    pub input_text_area: TextArea,
    pub config: Config,
    pub should_quit: bool,
    // Review comments (inline comments + reviews)
    pub review_comments: Option<Vec<ReviewComment>>,
    pub selected_comment: usize,
    pub comment_list_scroll_offset: usize,
    pub comments_loading: bool,
    // Comment positions in current diff view
    pub file_comment_positions: Vec<CommentPosition>,
    // Set of diff line indices with comments (for fast lookup in render)
    pub file_comment_lines: HashSet<usize>,
    /// インラインコメントパネルが開いているか（= フォーカス中）
    pub comment_panel_open: bool,
    /// インラインコメントパネルのスクロールオフセット（行単位）
    pub comment_panel_scroll: u16,
    // Cached diff lines (syntax highlighted)
    pub diff_cache: Option<DiffCache>,
    // Store for highlighted diff caches (file_index -> DiffCache)
    highlighted_cache_store: HashMap<usize, DiffCache>,
    // Discussion comments (PR conversation)
    pub discussion_comments: Option<Vec<DiscussionComment>>,
    pub selected_discussion_comment: usize,
    pub discussion_comment_list_scroll_offset: usize,
    pub discussion_comments_loading: bool,
    pub discussion_comment_detail_mode: bool,
    pub discussion_comment_detail_scroll: usize,
    /// ヘルプ画面のスクロールオフセット（行単位）
    pub help_scroll_offset: usize,
    // Comment tab state
    pub comment_tab: CommentTab,
    // AI Rally state
    pub ai_rally_state: Option<AiRallyState>,
    pub working_dir: Option<String>,
    // Receivers
    // PR-specific receivers carry the originating PR number to avoid
    // cross-PR cache contamination when the user switches PRs mid-flight.
    data_receiver: PrReceiver<DataLoadResult>,
    retry_sender: Option<mpsc::Sender<RefreshRequest>>,
    comment_receiver: PrReceiver<Result<Vec<ReviewComment>, String>>,
    diff_cache_receiver: Option<mpsc::Receiver<DiffCache>>,
    prefetch_receiver: Option<mpsc::Receiver<DiffCache>>,
    discussion_comment_receiver: PrReceiver<Result<Vec<DiscussionComment>, String>>,
    rally_event_receiver: Option<mpsc::Receiver<RallyEvent>>,
    // Handle for aborting the rally orchestrator task
    rally_abort_handle: Option<AbortHandle>,
    // Command sender to communicate with the orchestrator
    rally_command_sender: Option<mpsc::Sender<OrchestratorCommand>>,
    // Flag to start AI Rally when data is loaded (set by --ai-rally CLI flag)
    start_ai_rally_on_load: bool,
    // Pending AI Rally flag (set when --ai-rally is passed with PR list mode)
    pending_ai_rally: bool,
    // Comment submission state
    comment_submit_receiver: PrReceiver<CommentSubmitResult>,
    // File viewed-state mutation results
    mark_viewed_receiver: PrReceiver<MarkViewedResult>,
    comment_submitting: bool,
    /// Last submission result: (success, message)
    pub submission_result: Option<(bool, String)>,
    /// Timestamp when result was set (for auto-hide)
    submission_result_time: Option<Instant>,
    /// Approve confirmation: holds the review body (empty string = no comment, Some(text) = with comment).
    pending_approve_body: Option<String>,
    /// Spinner animation frame counter (incremented each tick)
    pub spinner_frame: usize,
    /// インラインコメントパネル内の選択インデックス
    pub selected_inline_comment: usize,
    /// ジャンプ履歴スタック（Go to Definition / Jump Back 用）
    pub jump_stack: Vec<JumpLocation>,
    /// Pending keys for multi-key sequences (e.g., "gg", "gd")
    pub pending_keys: SmallVec<[KeyBinding; 4]>,
    /// Timestamp when pending keys started (for timeout)
    pub pending_since: Option<Instant>,
    /// シンボル選択ポップアップの状態
    pub symbol_popup: Option<SymbolPopupState>,
    /// インメモリセッションキャッシュ
    pub session_cache: SessionCache,
    /// Markdown リッチ表示モード（見出し太字・斜体等を適用）
    markdown_rich: bool,
    /// PR一覧のキーワードフィルタ
    pub pr_list_filter: Option<ListFilter>,
    /// ファイル一覧のキーワードフィルタ
    pub file_list_filter: Option<ListFilter>,
    /// BG バッチ diff ロード結果の受信チャネル（Phase 2）
    batch_diff_receiver: Option<mpsc::Receiver<Vec<SingleFileDiffResult>>>,
    /// 単一ファイル diff のオンデマンド受信チャネル
    lazy_diff_receiver: Option<mpsc::Receiver<SingleFileDiffResult>>,
    /// 現在オンデマンドロード要求中のファイル名（重複リクエスト防止）
    lazy_diff_pending_file: Option<String>,
}

impl App {
    /// Loading状態で開始
    pub fn new_loading(
        repo: &str,
        pr_number: u32,
        config: Config,
    ) -> (Self, mpsc::Sender<DataLoadResult>) {
        let (tx, rx) = mpsc::channel(2);

        let app = Self {
            repo: repo.to_string(),
            pr_number: Some(pr_number),
            data_state: DataState::Loading,
            state: AppState::FileList,
            pr_list: None,
            selected_pr: 0,
            pr_list_scroll_offset: 0,
            pr_list_loading: false,
            pr_list_has_more: false,
            pr_list_state_filter: PrStateFilter::default(),
            started_from_pr_list: false,
            local_mode: false,
            local_auto_focus: false,
            local_file_signatures: HashMap::new(),
            local_file_patch_signatures: HashMap::new(),
            original_pr_number: Some(pr_number),
            saved_pr_snapshot: None,
            saved_local_snapshot: None,
            watcher_handle: None,
            refresh_pending: None,
            pr_list_receiver: None,
            diff_view_return_state: AppState::FileList,
            preview_return_state: AppState::DiffView,
            previous_state: AppState::FileList,
            selected_file: 0,
            file_list_scroll_offset: 0,
            selected_line: 0,
            diff_line_count: 0,
            scroll_offset: 0,
            multiline_selection: None,
            input_mode: None,
            input_text_area: TextArea::with_submit_key(config.keybindings.submit.clone()),
            config,
            should_quit: false,
            review_comments: None,
            selected_comment: 0,
            comment_list_scroll_offset: 0,
            comments_loading: false,
            file_comment_positions: vec![],
            file_comment_lines: HashSet::new(),
            comment_panel_open: false,
            comment_panel_scroll: 0,
            diff_cache: None,
            highlighted_cache_store: HashMap::new(),
            discussion_comments: None,
            selected_discussion_comment: 0,
            discussion_comment_list_scroll_offset: 0,
            discussion_comments_loading: false,
            discussion_comment_detail_mode: false,
            discussion_comment_detail_scroll: 0,
            help_scroll_offset: 0,
            comment_tab: CommentTab::default(),
            ai_rally_state: None,
            working_dir: None,
            data_receiver: Some((pr_number, rx)),
            retry_sender: None,
            comment_receiver: None,
            diff_cache_receiver: None,
            prefetch_receiver: None,
            discussion_comment_receiver: None,
            rally_event_receiver: None,
            rally_abort_handle: None,
            rally_command_sender: None,
            start_ai_rally_on_load: false,
            pending_ai_rally: false,
            comment_submit_receiver: None,
            mark_viewed_receiver: None,
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
            pending_approve_body: None,
            spinner_frame: 0,
            selected_inline_comment: 0,
            jump_stack: Vec::new(),
            pending_keys: SmallVec::new(),
            pending_since: None,
            symbol_popup: None,
            session_cache: SessionCache::new(),
            markdown_rich: false,
            pr_list_filter: None,
            file_list_filter: None,
            batch_diff_receiver: None,
            lazy_diff_receiver: None,
            lazy_diff_pending_file: None,
        };

        (app, tx)
    }

    /// PR一覧表示モードで開始（--pr省略時）
    pub fn new_pr_list(repo: &str, config: Config) -> Self {
        Self {
            repo: repo.to_string(),
            pr_number: None,
            data_state: DataState::Loading,
            state: AppState::PullRequestList,
            pr_list: None,
            selected_pr: 0,
            pr_list_scroll_offset: 0,
            pr_list_loading: true,
            pr_list_has_more: false,
            pr_list_state_filter: PrStateFilter::default(),
            started_from_pr_list: true,
            pr_list_receiver: None,
            diff_view_return_state: AppState::FileList,
            preview_return_state: AppState::DiffView,
            previous_state: AppState::PullRequestList,
            selected_file: 0,
            file_list_scroll_offset: 0,
            selected_line: 0,
            diff_line_count: 0,
            scroll_offset: 0,
            multiline_selection: None,
            input_mode: None,
            input_text_area: TextArea::with_submit_key(config.keybindings.submit.clone()),
            config,
            should_quit: false,
            review_comments: None,
            selected_comment: 0,
            comment_list_scroll_offset: 0,
            comments_loading: false,
            file_comment_positions: vec![],
            file_comment_lines: HashSet::new(),
            comment_panel_open: false,
            comment_panel_scroll: 0,
            diff_cache: None,
            highlighted_cache_store: HashMap::new(),
            discussion_comments: None,
            selected_discussion_comment: 0,
            discussion_comment_list_scroll_offset: 0,
            discussion_comments_loading: false,
            discussion_comment_detail_mode: false,
            discussion_comment_detail_scroll: 0,
            help_scroll_offset: 0,
            comment_tab: CommentTab::default(),
            ai_rally_state: None,
            working_dir: None,
            data_receiver: None,
            retry_sender: None,
            comment_receiver: None,
            diff_cache_receiver: None,
            prefetch_receiver: None,
            discussion_comment_receiver: None,
            rally_event_receiver: None,
            rally_abort_handle: None,
            rally_command_sender: None,
            start_ai_rally_on_load: false,
            pending_ai_rally: false,
            comment_submit_receiver: None,
            mark_viewed_receiver: None,
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
            pending_approve_body: None,
            spinner_frame: 0,
            selected_inline_comment: 0,
            jump_stack: Vec::new(),
            pending_keys: SmallVec::new(),
            pending_since: None,
            symbol_popup: None,
            local_mode: false,
            local_auto_focus: false,
            local_file_signatures: HashMap::new(),
            local_file_patch_signatures: HashMap::new(),
            original_pr_number: None,
            saved_pr_snapshot: None,
            saved_local_snapshot: None,
            watcher_handle: None,
            refresh_pending: None,
            session_cache: SessionCache::new(),
            markdown_rich: false,
            pr_list_filter: None,
            file_list_filter: None,
            batch_diff_receiver: None,
            lazy_diff_receiver: None,
            lazy_diff_pending_file: None,
        }
    }

    /// PR一覧受信チャンネルを設定
    pub fn set_pr_list_receiver(&mut self, rx: mpsc::Receiver<Result<github::PrListPage, String>>) {
        self.pr_list_receiver = Some(rx);
    }

    /// データ受信チャンネルを設定
    pub fn set_data_receiver(&mut self, pr_number: u32, rx: mpsc::Receiver<DataLoadResult>) {
        self.data_receiver = Some((pr_number, rx));
    }

    pub fn set_retry_sender(&mut self, tx: mpsc::Sender<RefreshRequest>) {
        self.retry_sender = Some(tx);
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = ui::setup_terminal()?;

        // データが既にロード済み（キャッシュヒット）の場合、プリフェッチを開始
        if matches!(self.data_state, DataState::Loaded { .. }) {
            self.start_prefetch_all_files();
        }

        // Start AI Rally immediately if flag is set and data is already loaded (from cache)
        if self.start_ai_rally_on_load && matches!(self.data_state, DataState::Loaded { .. }) {
            self.start_ai_rally_on_load = false;
            self.start_ai_rally();
        }

        while !self.should_quit {
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
            self.poll_pr_list_updates();
            self.poll_data_updates();
            self.poll_comment_updates();
            self.poll_diff_cache_updates();
            self.poll_prefetch_updates();
            self.poll_batch_diff_updates();
            self.poll_lazy_diff_updates();
            self.poll_discussion_comment_updates();
            self.poll_comment_submit_updates();
            self.poll_mark_viewed_updates();
            self.poll_rally_events();
            terminal.draw(|frame| ui::render(frame, self))?;
            self.handle_input(&mut terminal).await?;
        }

        // Graceful shutdown: abort any running rally
        if let Some(handle) = self.rally_abort_handle.take() {
            handle.abort();
        }

        ui::restore_terminal(&mut terminal)?;
        Ok(())
    }

    /// Get the current spinner character for loading animations
    pub fn spinner_char(&self) -> &str {
        SPINNER_FRAMES[self.spinner_frame % SPINNER_FRAMES.len()]
    }

    pub fn set_working_dir(&mut self, dir: Option<String>) {
        self.working_dir = dir;
    }

    pub fn set_local_mode(&mut self, local: bool) {
        self.local_mode = local;
    }

    pub fn set_local_auto_focus(&mut self, enable: bool) {
        self.local_auto_focus = enable;
    }

    pub fn is_local_mode(&self) -> bool {
        self.local_mode
    }

    pub fn is_local_auto_focus(&self) -> bool {
        self.local_auto_focus
    }

    pub fn is_markdown_rich(&self) -> bool {
        self.markdown_rich
    }

    /// Set flag to start AI Rally when data is loaded (used by --ai-rally CLI flag)
    pub fn set_start_ai_rally_on_load(&mut self, start: bool) {
        self.start_ai_rally_on_load = start;
    }

    /// Set pending AI Rally flag (for PR list mode with --ai-rally)
    pub fn set_pending_ai_rally(&mut self, pending: bool) {
        self.pending_ai_rally = pending;
    }

    /// PR番号を取得（未設定の場合はpanic）
    /// PR一覧から選択後は必ず設定されている前提
    pub fn pr_number(&self) -> u32 {
        self.pr_number
            .expect("pr_number should be set before accessing PR data")
    }

    /// コメント送信中かどうか
    pub fn is_submitting_comment(&self) -> bool {
        self.comment_submitting
    }

    /// Approve confirmation prompt is active.
    pub fn is_pending_approve_confirmation(&self) -> bool {
        self.pending_approve_body.is_some()
    }

    /// Build dynamic footer text for approve confirmation prompt.
    pub fn approve_confirmation_footer_text(&self) -> String {
        let kb = &self.config.keybindings;
        format!(
            "{}: confirm approve | {}/Esc: cancel",
            kb.approve.display(),
            kb.quit.display(),
        )
    }

    pub fn new_for_test() -> Self {
        let config = Config::default();
        Self {
            repo: "test/repo".to_string(),
            pr_number: Some(1),
            data_state: DataState::Loading,
            state: AppState::FileList,
            pr_list: None,
            selected_pr: 0,
            pr_list_scroll_offset: 0,
            pr_list_loading: false,
            pr_list_has_more: false,
            pr_list_state_filter: PrStateFilter::default(),
            started_from_pr_list: false,
            pr_list_receiver: None,
            diff_view_return_state: AppState::FileList,
            preview_return_state: AppState::DiffView,
            previous_state: AppState::FileList,
            selected_file: 0,
            file_list_scroll_offset: 0,
            selected_line: 0,
            diff_line_count: 0,
            scroll_offset: 0,
            multiline_selection: None,
            input_mode: None,
            input_text_area: TextArea::with_submit_key(config.keybindings.submit.clone()),
            config,
            should_quit: false,
            review_comments: None,
            selected_comment: 0,
            comment_list_scroll_offset: 0,
            comments_loading: false,
            file_comment_positions: vec![],
            file_comment_lines: HashSet::new(),
            comment_panel_open: false,
            comment_panel_scroll: 0,
            diff_cache: None,
            highlighted_cache_store: HashMap::new(),
            discussion_comments: None,
            selected_discussion_comment: 0,
            discussion_comment_list_scroll_offset: 0,
            discussion_comments_loading: false,
            discussion_comment_detail_mode: false,
            discussion_comment_detail_scroll: 0,
            help_scroll_offset: 0,
            comment_tab: CommentTab::default(),
            ai_rally_state: None,
            working_dir: None,
            data_receiver: None,
            retry_sender: None,
            comment_receiver: None,
            diff_cache_receiver: None,
            prefetch_receiver: None,
            discussion_comment_receiver: None,
            rally_event_receiver: None,
            rally_abort_handle: None,
            rally_command_sender: None,
            start_ai_rally_on_load: false,
            pending_ai_rally: false,
            comment_submit_receiver: None,
            mark_viewed_receiver: None,
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
            pending_approve_body: None,
            spinner_frame: 0,
            selected_inline_comment: 0,
            jump_stack: Vec::new(),
            pending_keys: SmallVec::new(),
            pending_since: None,
            symbol_popup: None,
            session_cache: SessionCache::new(),
            local_mode: false,
            local_auto_focus: false,
            local_file_signatures: HashMap::new(),
            local_file_patch_signatures: HashMap::new(),
            original_pr_number: None,
            saved_pr_snapshot: None,
            saved_local_snapshot: None,
            watcher_handle: None,
            refresh_pending: None,
            markdown_rich: false,
            pr_list_filter: None,
            file_list_filter: None,
            batch_diff_receiver: None,
            lazy_diff_receiver: None,
            lazy_diff_pending_file: None,
        }
    }

    /// Set the comment_submitting flag for testing.
    #[cfg(test)]
    pub fn set_submitting_for_test(&mut self, submitting: bool) {
        self.comment_submitting = submitting;
    }

    #[cfg(test)]
    pub fn set_pending_approve_body_for_test(&mut self, body: Option<String>) {
        self.pending_approve_body = body;
    }
}
