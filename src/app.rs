use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use lasso::{Rodeo, Spur};
use ratatui::{backend::CrosstermBackend, style::Style, Terminal};
use smallvec::SmallVec;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::io::Stdout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::ai::orchestrator::{OrchestratorCommand, RallyEvent};
use crate::ai::{Context, Orchestrator, RallyState};
use crate::cache::{PrCacheKey, PrData, SessionCache};
use crate::config::Config;
use crate::github::comment::{DiscussionComment, ReviewComment};
use crate::github::{self, ChangedFile, PrStateFilter, PullRequest, PullRequestSummary};
use crate::keybinding::{
    event_to_keybinding, KeyBinding, KeySequence, SequenceMatch, SEQUENCE_TIMEOUT,
};
use crate::loader::{CommentSubmitResult, DataLoadResult};
use crate::syntax::ParserPool;
use crate::ui;
use crate::ui::text_area::{TextArea, TextAreaAction};
use notify::Watcher;
use std::time::Instant;

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

/// コメントのdiff内位置を表す構造体
#[derive(Debug, Clone)]
pub struct CommentPosition {
    pub diff_line_index: usize,
    pub comment_index: usize,
}

/// ジャンプ履歴の1エントリ（Go to Definition / Jump Back 用）
#[derive(Debug, Clone)]
pub struct JumpLocation {
    pub file_index: usize,
    pub line_index: usize,
    pub scroll_offset: usize,
}

/// シンボル選択ポップアップの状態
#[derive(Debug, Clone)]
pub struct SymbolPopupState {
    /// 候補シンボル一覧 (name, start, end)
    pub symbols: Vec<(String, usize, usize)>,
    /// 選択中のインデックス
    pub selected: usize,
}

/// インターン済みの Span（アロケーション削減）
///
/// 文字列をインターナーに格納し、4バイトの Spur で参照することで
/// 重複トークンのアロケーションを削減する。
#[derive(Clone)]
pub struct InternedSpan {
    /// インターン済み文字列への参照（4 bytes）
    pub content: Spur,
    /// スタイル情報（8 bytes）
    pub style: Style,
}

/// Diff行のキャッシュ（シンタックスハイライト済み）
#[derive(Clone)]
pub struct CachedDiffLine {
    /// 基本の Span（REVERSED なし）
    pub spans: Vec<InternedSpan>,
}

/// Diff表示のキャッシュ
pub struct DiffCache {
    /// キャッシュ対象のファイルインデックス
    pub file_index: usize,
    /// patch のハッシュ（変更検出用）
    pub patch_hash: u64,
    /// パース済みの行データ
    pub lines: Vec<CachedDiffLine>,
    /// 文字列インターナー（キャッシュ内で共有）
    pub interner: Rodeo,
    /// シンタックスハイライト済みかどうか（プレーンキャッシュは false）
    pub highlighted: bool,
    /// Markdown リッチ表示モードで構築されたかどうか
    pub markdown_rich: bool,
}

impl DiffCache {
    /// Spur を文字列参照に解決する
    ///
    /// ライフタイムは DiffCache に依存するため、ゼロコピーでレンダリング可能。
    pub fn resolve(&self, spur: Spur) -> &str {
        self.interner.resolve(&spur)
    }
}

/// 文字列のハッシュを計算
pub fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// 行ベース入力のコンテキスト（コメント/サジェスチョン共通）
#[derive(Debug, Clone)]
pub struct LineInputContext {
    pub file_index: usize,
    pub line_number: u32,
    /// patch 内の position（1始まり）。GitHub API の `position` パラメータに対応。
    pub diff_position: u32,
}

/// 統一入力モード
#[derive(Debug, Clone)]
pub enum InputMode {
    Comment(LineInputContext),
    Suggestion {
        context: LineInputContext,
        original_code: String,
    },
    Reply {
        comment_id: u64,
        reply_to_user: String,
        reply_to_body: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    PullRequestList,
    FileList,
    DiffView,
    TextInput,
    CommentList,
    Help,
    AiRally,
    SplitViewFileList,
    SplitViewDiff,
}

/// Variant for diff view handling (fullscreen vs split pane)
#[derive(Debug, Clone, Copy, PartialEq)]
enum DiffViewVariant {
    /// Fullscreen diff view
    Fullscreen,
    /// Split pane diff view (right pane)
    SplitPane,
}

/// Log event type for AI Rally
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEventType {
    Info,
    Thinking,
    ToolUse,
    ToolResult,
    Text,
    Review,
    Fix,
    Error,
}

/// Structured log entry for AI Rally
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub event_type: LogEventType,
    pub message: String,
}

impl LogEntry {
    pub fn new(event_type: LogEventType, message: String) -> Self {
        let now = chrono::Local::now();
        Self {
            timestamp: now.format("%H:%M:%S").to_string(),
            event_type,
            message,
        }
    }
}

/// Permission request information for AI Rally
#[derive(Debug, Clone)]
pub struct PermissionInfo {
    pub action: String,
    pub reason: String,
}

/// State for AI Rally view
#[derive(Debug, Clone)]
pub struct AiRallyState {
    pub iteration: u32,
    pub max_iterations: u32,
    pub state: RallyState,
    pub history: Vec<RallyEvent>,
    pub logs: Vec<LogEntry>,
    pub log_scroll_offset: usize,
    /// Selected log index for detail view
    pub selected_log_index: Option<usize>,
    /// Whether the log detail modal is visible
    pub showing_log_detail: bool,
    /// Pending clarification question from reviewee
    pub pending_question: Option<String>,
    /// Pending permission request from reviewee
    pub pending_permission: Option<PermissionInfo>,
    /// Pending review post confirmation
    pub pending_review_post: Option<crate::ai::orchestrator::ReviewPostInfo>,
    /// Pending fix post confirmation
    pub pending_fix_post: Option<crate::ai::orchestrator::FixPostInfo>,
    /// Last rendered visible log height (updated by UI render)
    pub last_visible_log_height: usize,
}

impl AiRallyState {
    /// Push a new log entry, auto-following to the bottom if the selection is at the tail.
    /// This keeps auto-scroll active when the user is watching the latest logs.
    pub fn push_log(&mut self, entry: LogEntry) {
        let was_at_tail = self.is_selection_at_tail();
        self.logs.push(entry);

        if was_at_tail {
            // Keep selection at the new tail and maintain auto-scroll
            self.selected_log_index = Some(self.logs.len().saturating_sub(1));
            self.log_scroll_offset = 0; // 0 means auto-scroll to bottom
        }
    }

    /// Check if the current selection is at the tail (last log) or unset
    fn is_selection_at_tail(&self) -> bool {
        match self.selected_log_index {
            None => true, // No selection = follow tail
            Some(idx) => {
                // At tail if selected index is the last log (or beyond)
                idx >= self.logs.len().saturating_sub(1)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReviewAction {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CommentTab {
    #[default]
    Review,
    Discussion,
}

/// リトライリクエストの種類（統一リトライループで使用）
#[derive(Debug, Clone)]
pub enum RefreshRequest {
    PrRefresh { pr_number: u32 },
    LocalRefresh,
}

/// ファイルウォッチャーのハンドル
///
/// `active` フラグで callback の処理を制御する。
/// スレッド自体は `_thread` で保持され、プロセス終了まで生存する。
pub struct WatcherHandle {
    active: Arc<AtomicBool>,
    _thread: std::thread::JoinHandle<()>,
}

/// モード切替時のビュー状態スナップショット
///
/// データは `SessionCache` で管理するため、ここには UI 状態のみ保持。
/// 全フィールドを `std::mem::replace` / `take()` で移動（Clone 不使用）。
pub struct ViewSnapshot {
    pub pr_number: Option<u32>,
    pub selected_file: usize,
    pub file_list_scroll_offset: usize,
    pub selected_line: usize,
    pub scroll_offset: usize,
    pub diff_cache: Option<DiffCache>,
    pub highlighted_cache_store: HashMap<usize, DiffCache>,
    pub review_comments: Option<Vec<ReviewComment>>,
    pub discussion_comments: Option<Vec<DiscussionComment>>,
    pub local_file_signatures: HashMap<String, u64>,
}

/// PRデータの読み込み状態。
///
/// `Loaded` のフィールドは `Arc` ではなく `Box`/`Vec` で保持する。
/// `SessionCache` との間のデータ分配は `clone()` で行う（PR更新時のみ発生）。
/// シングルスレッドで完結する設計のため、`Arc` による共有所有権は不要。
#[derive(Debug, Clone)]
pub enum DataState {
    Loading,
    Loaded {
        pr: Box<PullRequest>,
        files: Vec<ChangedFile>,
    },
    Error(String),
}

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
    /// 直近のローカルファイル署名（差分変更を検出）
    local_file_signatures: HashMap<String, u64>,
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
    comment_submitting: bool,
    /// Last submission result: (success, message)
    pub submission_result: Option<(bool, String)>,
    /// Timestamp when result was set (for auto-hide)
    submission_result_time: Option<Instant>,
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
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
            spinner_frame: 0,
            selected_inline_comment: 0,
            jump_stack: Vec::new(),
            pending_keys: SmallVec::new(),
            pending_since: None,
            symbol_popup: None,
            session_cache: SessionCache::new(),
            markdown_rich: false,
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
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
            spinner_frame: 0,
            selected_inline_comment: 0,
            jump_stack: Vec::new(),
            pending_keys: SmallVec::new(),
            pending_since: None,
            symbol_popup: None,
            local_mode: false,
            local_auto_focus: false,
            local_file_signatures: HashMap::new(),
            original_pr_number: None,
            saved_pr_snapshot: None,
            saved_local_snapshot: None,
            watcher_handle: None,
            refresh_pending: None,
            session_cache: SessionCache::new(),
            markdown_rich: false,
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
            self.poll_discussion_comment_updates();
            self.poll_comment_submit_updates();
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

    fn toggle_markdown_rich(&mut self) {
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

    fn toggle_local_mode(&mut self) {
        // フォアグラウンド Rally 中はブロック
        if matches!(self.state, AppState::AiRally) {
            self.submission_result =
                Some((false, "Cannot toggle mode during AI Rally".to_string()));
            self.submission_result_time = Some(Instant::now());
            return;
        }

        if self.local_mode {
            // Local → PR
            self.deactivate_watcher();
            self.saved_local_snapshot = Some(self.save_view_snapshot());
            self.local_mode = false;

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
    fn update_data_receiver_origin(&mut self, pr_number: u32) {
        if let Some((ref mut origin, _)) = self.data_receiver {
            *origin = pr_number;
        }
    }

    /// SessionCache からデータを復元し、ない場合は Loading + retry_load
    fn restore_data_from_cache(&mut self) {
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
    fn detect_local_base_branch(working_dir: Option<&str>) -> Option<String> {
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
    fn save_view_snapshot(&mut self) -> ViewSnapshot {
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
        }
    }

    /// スナップショットから UI 状態を復元（O(1) 移動）
    ///
    /// channel は触らない（永続チャンネルのため）。
    /// データは `SessionCache` から別途取得する。
    fn restore_view_snapshot(&mut self, snapshot: ViewSnapshot) {
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
    fn activate_watcher(&mut self) {
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
    fn deactivate_watcher(&mut self) {
        if let Some(ref handle) = self.watcher_handle {
            handle.active.store(false, Ordering::Release);
        }
    }

    fn toggle_auto_focus(&mut self) {
        self.local_auto_focus = !self.local_auto_focus;
        let msg = if self.local_auto_focus {
            "Auto-focus: ON"
        } else {
            "Auto-focus: OFF"
        };
        self.submission_result = Some((true, msg.to_string()));
        self.submission_result_time = Some(Instant::now());
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

    /// PR一覧取得のポーリング
    fn poll_pr_list_updates(&mut self) {
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
    fn poll_data_updates(&mut self) {
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
    fn poll_comment_updates(&mut self) {
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
    fn poll_diff_cache_updates(&mut self) {
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
    fn start_prefetch_all_files(&mut self) {
        // 既存のプリフェッチを中断
        self.prefetch_receiver = None;

        // キャッシュ済みファイルをスキップし、上限まで収集
        let files: Vec<_> = self
            .files()
            .iter()
            .enumerate()
            .filter(|(i, f)| f.patch.is_some() && !self.highlighted_cache_store.contains_key(i))
            .take(MAX_PREFETCH_FILES)
            .map(|(i, f)| (i, f.filename.clone(), f.patch.clone().unwrap()))
            .collect();

        if files.is_empty() {
            return;
        }

        let theme = self.config.diff.theme.clone();
        let markdown_rich = self.markdown_rich;
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
                );
                cache.file_index = *index;
                if tx.blocking_send(cache).is_err() {
                    break; // receiver がドロップされた
                }
            }
        });
    }

    /// プリフェッチ結果をポーリングして highlighted_cache_store に格納
    fn poll_prefetch_updates(&mut self) {
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
                    break;
                }
            }
        }
    }

    /// Discussion コメント取得のポーリング
    fn poll_discussion_comment_updates(&mut self) {
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
    fn poll_comment_submit_updates(&mut self) {
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

    /// コメント送信中かどうか
    pub fn is_submitting_comment(&self) -> bool {
        self.comment_submitting
    }

    /// AI Rally イベントのポーリング
    fn poll_rally_events(&mut self) {
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
                                    RallyState::Completed
                                        | RallyState::Aborted
                                        | RallyState::Error
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

    fn handle_data_result(&mut self, origin_pr: u32, result: DataLoadResult) {
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
                // selected_file が変更された場合、コメント位置キャッシュを再計算
                if self.selected_file != old_selected {
                    self.update_file_comment_positions();
                }
                // 全ファイルのハイライトキャッシュを事前構築
                self.start_prefetch_all_files();
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

    fn local_file_signature(file: &ChangedFile) -> u64 {
        let patch = file.patch.as_deref().unwrap_or_default();
        let signature = format!(
            "{}|{}|{}|{}|{}",
            file.filename, file.status, file.additions, file.deletions, patch
        );
        hash_string(&signature)
    }

    fn find_changed_local_file_index(
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

    fn remember_local_file_signatures(&mut self, files: &[ChangedFile]) {
        self.local_file_signatures.clear();
        for file in files {
            self.local_file_signatures
                .insert(file.filename.clone(), Self::local_file_signature(file));
        }
    }

    fn calc_diff_line_count(files: &[ChangedFile], selected: usize) -> usize {
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

    async fn handle_input(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // PR一覧画面は独自のLoading処理があるためスキップ
                if self.state != AppState::PullRequestList {
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
                }

                match self.state {
                    AppState::PullRequestList => self.handle_pr_list_input(key).await?,
                    AppState::FileList => self.handle_file_list_input(key, terminal).await?,
                    AppState::DiffView => self.handle_diff_view_input(key, terminal).await?,
                    AppState::TextInput => self.handle_text_input(key)?,
                    AppState::CommentList => self.handle_comment_list_input(key, terminal).await?,
                    AppState::Help => self.handle_help_input(key)?,
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

    fn retry_load(&mut self) {
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

    async fn handle_file_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let kb = &self.config.keybindings;

        // Quit or back to PR list
        if self.matches_single_key(&key, &kb.quit) {
            if self.started_from_pr_list {
                self.back_to_pr_list();
            } else {
                self.should_quit = true;
            }
            return Ok(());
        }

        // Move down (j or Down arrow - arrows always work)
        if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
            if !self.files().is_empty() {
                self.selected_file =
                    (self.selected_file + 1).min(self.files().len().saturating_sub(1));
            }
            return Ok(());
        }

        // Move up (k or Up arrow)
        if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
            self.selected_file = self.selected_file.saturating_sub(1);
            return Ok(());
        }

        // Open split view (Enter, Right arrow, or l)
        if self.matches_single_key(&key, &kb.open_panel)
            || self.matches_single_key(&key, &kb.move_right)
            || key.code == KeyCode::Right
        {
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
            return Ok(());
        }

        Ok(())
    }

    /// FileList 系状態で共通のキーを処理する。処理した場合は true を返す。
    async fn handle_common_file_list_keys(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<bool> {
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

    async fn handle_split_view_file_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let kb = &self.config.keybindings;

        // Move down
        if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
            if !self.files().is_empty() {
                self.selected_file =
                    (self.selected_file + 1).min(self.files().len().saturating_sub(1));
                self.sync_diff_to_selected_file();
            }
            return Ok(());
        }

        // Move up
        if self.matches_single_key(&key, &kb.move_up) || key.code == KeyCode::Up {
            if self.selected_file > 0 {
                self.selected_file = self.selected_file.saturating_sub(1);
                self.sync_diff_to_selected_file();
            }
            return Ok(());
        }

        // Focus diff pane
        if self.matches_single_key(&key, &kb.open_panel)
            || self.matches_single_key(&key, &kb.move_right)
            || key.code == KeyCode::Right
        {
            if !self.files().is_empty() {
                self.state = AppState::SplitViewDiff;
            }
            return Ok(());
        }

        // Back to file list
        if self.matches_single_key(&key, &kb.quit)
            || self.matches_single_key(&key, &kb.move_left)
            || key.code == KeyCode::Left
            || key.code == KeyCode::Esc
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
            return Ok(());
        }

        // Fallback to common file list keys
        self.handle_common_file_list_keys(key, terminal).await?;

        Ok(())
    }

    /// Common handler for diff view input (both fullscreen and split pane)
    ///
    /// The `variant` parameter determines:
    /// - visible_lines calculation
    /// - state transitions (back, quit, panel navigation)
    async fn handle_diff_input_common(
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
        if self.matches_single_key(&key, &kb.page_down) {
            if self.diff_line_count > 0 {
                self.selected_line =
                    (self.selected_line + 20).min(self.diff_line_count.saturating_sub(1));
                self.adjust_scroll(visible_lines);
            }
            return Ok(());
        }

        // Page up
        if self.matches_single_key(&key, &kb.page_up) {
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

    async fn handle_split_view_diff_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        self.handle_diff_input_common(key, terminal, DiffViewVariant::SplitPane)
            .await
    }

    async fn handle_ai_rally_input(
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

    /// Adjust log scroll offset to keep the selected log visible
    fn adjust_log_scroll_to_selection(&mut self) {
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

    /// Send a command to the orchestrator
    fn send_rally_command(&mut self, cmd: OrchestratorCommand) {
        if let Some(ref sender) = self.rally_command_sender {
            // Use try_send since we're not in an async context
            if sender.try_send(cmd).is_err() {
                // Orchestrator may have terminated, clean up state
                self.cleanup_rally_state();
            }
        }
    }

    /// Clean up rally state when orchestrator terminates or user aborts
    fn cleanup_rally_state(&mut self) {
        self.ai_rally_state = None;
        self.rally_command_sender = None;
        self.rally_event_receiver = None;
        if let Some(handle) = self.rally_abort_handle.take() {
            handle.abort();
        }
    }

    /// Open editor for clarification input synchronously
    fn open_clarification_editor_sync(
        &mut self,
        question: &str,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // Restore terminal before opening editor
        ui::restore_terminal(terminal)?;

        // Open editor (blocking)
        let answer = crate::editor::open_clarification_editor(self.config.editor.as_deref(), question)?;

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

    /// 既存のRallyがあれば画面遷移のみ、なければ新規Rally開始
    fn resume_or_start_ai_rally(&mut self) {
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

    fn start_ai_rally(&mut self) {
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

    fn refresh_all(&mut self) {
        // インメモリキャッシュを全削除
        self.session_cache.invalidate_all();
        // コメントデータをクリア
        self.review_comments = None;
        self.discussion_comments = None;
        self.comments_loading = false;
        self.discussion_comments_loading = false;
        // 強制的に Loading 状態にしてから再取得
        self.data_state = DataState::Loading;
        self.retry_load();
    }

    fn open_pr_in_browser(&self, pr_number: u32) {
        let repo = self.repo.clone();
        tokio::spawn(async move {
            let _ =
                github::gh_command(&["pr", "view", &pr_number.to_string(), "-R", &repo, "--web"])
                    .await;
        });
    }

    async fn handle_diff_view_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        self.handle_diff_input_common(key, terminal, DiffViewVariant::Fullscreen)
            .await
    }

    fn adjust_scroll(&mut self, visible_lines: usize) {
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

    /// 統一入力ハンドラー（コメント/サジェスチョン/リプライ共通）
    fn handle_text_input(&mut self, key: event::KeyEvent) -> Result<()> {
        // 送信中は入力を無視
        if self.comment_submitting {
            return Ok(());
        }

        match self.input_text_area.input(key) {
            TextAreaAction::Submit => {
                let content = self.input_text_area.content();
                if content.trim().is_empty() {
                    // 空の場合はキャンセル扱い
                    self.cancel_input();
                    return Ok(());
                }

                match self.input_mode.take() {
                    Some(InputMode::Comment(ctx)) => {
                        self.submit_comment(ctx, content);
                    }
                    Some(InputMode::Suggestion {
                        context,
                        original_code: _,
                    }) => {
                        self.submit_suggestion(context, content);
                    }
                    Some(InputMode::Reply { comment_id, .. }) => {
                        self.submit_reply(comment_id, content);
                    }
                    None => {}
                }
                self.state = self.preview_return_state;
            }
            TextAreaAction::Cancel => {
                self.cancel_input();
            }
            TextAreaAction::Continue => {}
            TextAreaAction::PendingSequence => {
                // Waiting for more keys in a sequence, do nothing
            }
        }
        Ok(())
    }

    fn cancel_input(&mut self) {
        self.input_mode = None;
        self.input_text_area.clear();
        self.state = self.preview_return_state;
    }

    fn submit_comment(&mut self, ctx: LineInputContext, body: String) {
        let Some(file) = self.files().get(ctx.file_index) else {
            return;
        };
        let Some(pr) = self.pr() else {
            return;
        };

        let commit_id = pr.head.sha.clone();
        let filename = file.filename.clone();
        let repo = self.repo.clone();
        let pr_number = self.pr_number();
        let position = ctx.diff_position;

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some((pr_number, rx));
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = github::create_review_comment(
                &repo, pr_number, &commit_id, &filename, position, &body,
            )
            .await;

            let _ = tx
                .send(match result {
                    Ok(_) => CommentSubmitResult::Success,
                    Err(e) => CommentSubmitResult::Error(e.to_string()),
                })
                .await;
        });
    }

    fn submit_suggestion(&mut self, ctx: LineInputContext, suggested_code: String) {
        let Some(file) = self.files().get(ctx.file_index) else {
            return;
        };
        let Some(pr) = self.pr() else {
            return;
        };

        let commit_id = pr.head.sha.clone();
        let filename = file.filename.clone();
        let body = format!("```suggestion\n{}\n```", suggested_code.trim_end());
        let repo = self.repo.clone();
        let pr_number = self.pr_number();
        let position = ctx.diff_position;

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some((pr_number, rx));
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = github::create_review_comment(
                &repo, pr_number, &commit_id, &filename, position, &body,
            )
            .await;

            let _ = tx
                .send(match result {
                    Ok(_) => CommentSubmitResult::Success,
                    Err(e) => CommentSubmitResult::Error(e.to_string()),
                })
                .await;
        });
    }

    fn submit_reply(&mut self, comment_id: u64, body: String) {
        let repo = self.repo.clone();
        let pr_number = self.pr_number();

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some((pr_number, rx));
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = github::create_reply_comment(&repo, pr_number, comment_id, &body).await;

            let _ = tx
                .send(match result {
                    Ok(_) => CommentSubmitResult::Success,
                    Err(e) => CommentSubmitResult::Error(e.to_string()),
                })
                .await;
        });
    }

    fn handle_help_input(&mut self, key: event::KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                self.state = self.previous_state;
            }
            _ => {}
        }
        Ok(())
    }

    /// コメント入力を開始（組み込みTextArea）
    fn enter_comment_input(&mut self) {
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
        }));
        self.input_text_area.clear();
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }

    async fn submit_review(
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
            tracing::debug!("submit_review: body is None (cancelled)");
            self.submission_result = Some((false, "Review cancelled".to_string()));
            self.submission_result_time = Some(Instant::now());
            return Ok(());
        };

        tracing::debug!(body_len = body.len(), "submit_review: calling GitHub API");
        match github::submit_review(&self.repo, self.pr_number(), action, &body).await {
            Ok(()) => {
                let action_str = match action {
                    ReviewAction::Approve => "approved",
                    ReviewAction::RequestChanges => "changes requested",
                    ReviewAction::Comment => "commented",
                };
                tracing::debug!(action_str, "submit_review: success");
                self.submission_result =
                    Some((true, format!("Review submitted ({})", action_str)));
                self.submission_result_time = Some(Instant::now());
            }
            Err(e) => {
                tracing::debug!(%e, "submit_review: API failed");
                self.submission_result = Some((false, format!("Review failed: {}", e)));
                self.submission_result_time = Some(Instant::now());
            }
        }
        Ok(())
    }

    fn update_diff_line_count(&mut self) {
        self.diff_line_count = Self::calc_diff_line_count(self.files(), self.selected_file);
    }

    /// Split Viewでファイル選択変更時にdiff状態を同期
    fn sync_diff_to_selected_file(&mut self) {
        self.selected_line = 0;
        self.scroll_offset = 0;
        self.comment_panel_open = false;
        self.comment_panel_scroll = 0;
        self.clear_pending_keys();
        self.symbol_popup = None;
        self.update_diff_line_count();
        if !self.local_mode && self.review_comments.is_none() {
            self.load_review_comments();
        }
        self.update_file_comment_positions();
        self.ensure_diff_cache();
    }

    /// サジェスチョン入力を開始（組み込みTextArea）
    fn enter_suggestion_input(&mut self) {
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
            },
            original_code: original_code.clone(),
        });
        // サジェスチョンは元コードを初期値として設定
        self.input_text_area.set_content(&original_code);
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }

    fn open_comment_list(&mut self) {
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

    fn load_review_comments(&mut self) {
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

    fn load_discussion_comments(&mut self) {
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

    async fn handle_comment_list_input(
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

    fn handle_discussion_detail_input(
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

    fn jump_to_comment(&mut self) {
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
    fn update_file_comment_positions(&mut self) {
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
    fn find_diff_line_index(patch: &str, target_line: u32) -> Option<usize> {
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
    fn wrapped_line_count(text: &str, panel_width: usize) -> usize {
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
    fn comment_body_wrapped_lines(body: &str, panel_width: usize) -> usize {
        body.lines()
            .map(|line| Self::wrapped_line_count(line, panel_width))
            .sum::<usize>()
            .max(1) // 空の本文でも最低1行
    }

    /// コメントパネルのコンテンツ行数を計算（スクロール上限算出用）
    fn comment_panel_content_lines(&self, panel_inner_width: usize) -> usize {
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
    fn comment_panel_offset_for(&self, target: usize, panel_inner_width: usize) -> u16 {
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
    fn comment_panel_inner_width(&self, terminal_width: usize) -> usize {
        let panel_width = match self.state {
            AppState::SplitViewDiff => terminal_width * 65 / 100,
            _ => terminal_width,
        };
        panel_width.saturating_sub(2) // borders
    }

    /// コメントパネルのスクロール上限を計算
    fn max_comment_panel_scroll(&self, terminal_height: usize, terminal_width: usize) -> u16 {
        let panel_inner_width = self.comment_panel_inner_width(terminal_width);
        let content_lines = self.comment_panel_content_lines(panel_inner_width);
        // コメントパネルは全体高さの約40%（Header/Footer/borders分を差し引き）
        let panel_inner_height = (terminal_height.saturating_sub(8) * 40 / 100).max(1);
        content_lines.saturating_sub(panel_inner_height) as u16
    }

    /// Jump to next comment in the diff (no wrap-around, scroll to top)
    fn jump_to_next_comment(&mut self) {
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
    fn jump_to_prev_comment(&mut self) {
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

    /// 返信入力モードに遷移（統一TextArea）
    fn enter_reply_input(&mut self) {
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

    /// 現在位置をジャンプスタックに保存
    fn push_jump_location(&mut self) {
        let loc = JumpLocation {
            file_index: self.selected_file,
            line_index: self.selected_line,
            scroll_offset: self.scroll_offset,
        };
        self.jump_stack.push(loc);
        // 上限 100 件
        if self.jump_stack.len() > 100 {
            self.jump_stack.remove(0);
        }
    }

    /// ジャンプスタックから復元
    fn jump_back(&mut self) {
        let Some(loc) = self.jump_stack.pop() else {
            return;
        };

        let file_changed = self.selected_file != loc.file_index;
        self.selected_file = loc.file_index;
        self.selected_line = loc.line_index;
        self.scroll_offset = loc.scroll_offset;

        if file_changed {
            self.update_diff_line_count();
            self.update_file_comment_positions();
            self.ensure_diff_cache();
        }
    }

    /// シンボル選択ポップアップを開く
    async fn open_symbol_popup(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let file = match self.files().get(self.selected_file) {
            Some(f) => f,
            None => return Ok(()),
        };
        let patch = match file.patch.as_ref() {
            Some(p) => p,
            None => return Ok(()),
        };
        let info = match crate::diff::get_line_info(patch, self.selected_line) {
            Some(i) => i,
            None => return Ok(()),
        };

        let symbols = crate::symbol::extract_all_identifiers(&info.line_content);
        if symbols.is_empty() {
            return Ok(());
        }

        // 候補が1つだけの場合は直接ジャンプ（ポップアップ不要）
        if symbols.len() == 1 {
            let symbol_name = symbols[0].0.clone();
            self.jump_to_symbol_definition_async(&symbol_name, terminal)
                .await?;
            return Ok(());
        }

        self.symbol_popup = Some(SymbolPopupState {
            symbols,
            selected: 0,
        });
        Ok(())
    }

    /// ポップアップ内のキーハンドリング
    async fn handle_symbol_popup_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let popup = match self.symbol_popup.as_mut() {
            Some(p) => p,
            None => return Ok(()),
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                popup.selected = (popup.selected + 1).min(popup.symbols.len().saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                popup.selected = popup.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                let symbol_name = popup.symbols[popup.selected].0.clone();
                self.symbol_popup = None;
                self.jump_to_symbol_definition_async(&symbol_name, terminal)
                    .await?;
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.symbol_popup = None;
            }
            _ => {}
        }
        Ok(())
    }

    /// シンボルの定義元へジャンプ（diff パッチ内 → リポジトリ全体、非同期）
    async fn jump_to_symbol_definition_async(
        &mut self,
        symbol: &str,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // Phase 1: diff パッチ内を検索
        let files: Vec<crate::github::ChangedFile> = self.files().to_vec();
        if let Some((file_idx, line_idx)) =
            crate::symbol::find_definition_in_patches(symbol, &files, self.selected_file)
        {
            self.push_jump_location();
            let file_changed = self.selected_file != file_idx;
            self.selected_file = file_idx;
            self.selected_line = line_idx;
            self.scroll_offset = line_idx;

            if file_changed {
                self.update_diff_line_count();
                self.update_file_comment_positions();
                self.ensure_diff_cache();
            }
            return Ok(());
        }

        // Phase 2: ローカルリポジトリ全体を検索
        let repo_root = match &self.working_dir {
            Some(dir) => {
                let output = tokio::process::Command::new("git")
                    .args(["rev-parse", "--show-toplevel"])
                    .current_dir(dir)
                    .output()
                    .await;
                match output {
                    Ok(o) if o.status.success() => {
                        String::from_utf8_lossy(&o.stdout).trim().to_string()
                    }
                    _ => return Ok(()),
                }
            }
            None => return Ok(()),
        };

        let result =
            crate::symbol::find_definition_in_repo(symbol, std::path::Path::new(&repo_root)).await;
        if let Ok(Some((file_path, line_number))) = result {
            let full_path = std::path::Path::new(&repo_root).join(&file_path);
            let path_str = full_path.to_string_lossy().to_string();

            // ターミナルを一時停止して外部エディタを開く
            crate::ui::restore_terminal(terminal)?;
            let _ = crate::editor::open_file_at_line(self.config.editor.as_deref(), &path_str, line_number);
            *terminal = crate::ui::setup_terminal()?;
        }

        Ok(())
    }

    /// 現在のファイルを外部エディタで開く（gf キー）
    async fn open_current_file_in_editor(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let file = match self.files().get(self.selected_file) {
            Some(f) => f.clone(),
            None => return Ok(()),
        };

        // 行番号: new_line_number があれば使用、なければ 1
        let line_number = file.patch.as_ref().and_then(|patch| {
            crate::diff::get_line_info(patch, self.selected_line)
                .and_then(|info| info.new_line_number)
        });

        // リポジトリルート取得 → フルパス構築
        let full_path = match &self.working_dir {
            Some(dir) => {
                let output = tokio::process::Command::new("git")
                    .args(["rev-parse", "--show-toplevel"])
                    .current_dir(dir)
                    .output()
                    .await;
                match output {
                    Ok(o) if o.status.success() => {
                        let root = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        std::path::Path::new(&root)
                            .join(&file.filename)
                            .to_string_lossy()
                            .to_string()
                    }
                    _ => return Ok(()),
                }
            }
            None => return Ok(()),
        };

        // TUI 一時停止 → エディタ → TUI 復帰
        crate::ui::restore_terminal(terminal)?;
        let _ = crate::editor::open_file_at_line(
            self.config.editor.as_deref(),
            &full_path,
            line_number.unwrap_or(1) as usize,
        );
        *terminal = crate::ui::setup_terminal()?;

        Ok(())
    }

    /// Diffキャッシュを構築または再利用
    ///
    /// キャッシュの3段階ルックアップ:
    /// 1. 現在の diff_cache が有効 → そのまま使用
    /// 2. highlighted_cache_store にハイライト済みキャッシュがある → 即座に復元
    /// 3. キャッシュミス → プレーン構築（~1ms）+ バックグラウンドハイライト構築
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
        let mut plain_cache = crate::ui::diff_view::build_plain_diff_cache(&patch);
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
            );
            cache.file_index = file_index;
            let _ = tx.try_send(cache);
        });
    }

    // ========================================
    // Keybinding helpers
    // ========================================

    /// Check sequence timeout and clear pending keys if expired
    fn check_sequence_timeout(&mut self) {
        if let Some(since) = self.pending_since {
            if since.elapsed() > SEQUENCE_TIMEOUT {
                self.pending_keys.clear();
                self.pending_since = None;
            }
        }
    }

    /// Add a key to pending sequence
    fn push_pending_key(&mut self, key: KeyBinding) {
        if self.pending_keys.is_empty() {
            self.pending_since = Some(Instant::now());
        }
        self.pending_keys.push(key);
    }

    /// Clear pending keys
    fn clear_pending_keys(&mut self) {
        self.pending_keys.clear();
        self.pending_since = None;
    }

    /// Check if a KeyEvent matches a KeySequence (single-key sequences only)
    fn matches_single_key(&self, event: &KeyEvent, seq: &KeySequence) -> bool {
        if !seq.is_single() {
            return false;
        }
        if let Some(first) = seq.first() {
            first.matches(event)
        } else {
            false
        }
    }

    /// Try to match pending keys against a sequence.
    /// Returns SequenceMatch::Full if fully matched, Partial if prefix matches, None otherwise.
    fn try_match_sequence(&self, seq: &KeySequence) -> SequenceMatch {
        if self.pending_keys.is_empty() {
            return SequenceMatch::None;
        }

        let pending_len = self.pending_keys.len();
        let seq_len = seq.0.len();

        if pending_len > seq_len {
            return SequenceMatch::None;
        }

        // Check if pending keys match the prefix of the sequence
        for (i, pending) in self.pending_keys.iter().enumerate() {
            if *pending != seq.0[i] {
                return SequenceMatch::None;
            }
        }

        if pending_len == seq_len {
            SequenceMatch::Full
        } else {
            SequenceMatch::Partial
        }
    }

    /// Check if current key event starts or continues a sequence that could match the given sequence
    fn key_could_match_sequence(&self, event: &KeyEvent, seq: &KeySequence) -> bool {
        let Some(kb) = event_to_keybinding(event) else {
            return false;
        };

        // If no pending keys, check if this key matches the first key of sequence
        if self.pending_keys.is_empty() {
            if let Some(first) = seq.first() {
                return *first == kb;
            }
            return false;
        }

        // If we have pending keys, check if adding this key could complete or continue the sequence
        let pending_len = self.pending_keys.len();
        if pending_len >= seq.0.len() {
            return false;
        }

        // Check if pending keys match prefix and new key matches next position
        for (i, pending) in self.pending_keys.iter().enumerate() {
            if *pending != seq.0[i] {
                return false;
            }
        }

        seq.0
            .get(pending_len)
            .map(|expected| *expected == kb)
            .unwrap_or(false)
    }

    /// PR一覧画面のキー入力処理
    async fn handle_pr_list_input(&mut self, key: event::KeyEvent) -> Result<()> {
        // Clone keybindings to avoid borrow conflicts
        let kb = self.config.keybindings.clone();

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

        // Move down (j or Down arrow)
        if self.matches_single_key(&key, &kb.move_down) || key.code == KeyCode::Down {
            if pr_count > 0 {
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
            self.selected_pr = self.selected_pr.saturating_sub(1);
            return Ok(());
        }

        // gg/G シーケンス処理
        if let Some(kb_event) = event_to_keybinding(&key) {
            self.check_sequence_timeout();

            if !self.pending_keys.is_empty() {
                self.push_pending_key(kb_event);

                // gg: 先頭へ
                if self.try_match_sequence(&kb.jump_to_first) == SequenceMatch::Full {
                    self.clear_pending_keys();
                    self.selected_pr = 0;
                    return Ok(());
                }

                // マッチしなければペンディングをクリア
                self.clear_pending_keys();
            } else {
                // シーケンス開始チェック
                if self.key_could_match_sequence(&key, &kb.jump_to_first) {
                    self.push_pending_key(kb_event);
                    return Ok(());
                }
            }
        }

        // G: 末尾へ
        if self.matches_single_key(&key, &kb.jump_to_last) {
            if pr_count > 0 {
                self.selected_pr = pr_count.saturating_sub(1);
            }
            return Ok(());
        }

        // Enter: PR選択
        if self.matches_single_key(&key, &kb.open_panel) {
            if let Some(ref prs) = self.pr_list {
                if let Some(pr) = prs.get(self.selected_pr) {
                    self.select_pr(pr.number);
                }
            }
            return Ok(());
        }

        // ブラウザで開く（configurable、フィルターキーより先に評価）
        if self.matches_single_key(&key, &kb.open_in_browser) {
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
            return Ok(());
        }

        Ok(())
    }

    /// PR一覧を再読み込み
    fn reload_pr_list(&mut self) {
        // 既存のリストをクリアせず、ローディング状態のみ設定
        // これにより、ローディング中も既存のリストが表示される
        self.selected_pr = 0;
        self.pr_list_scroll_offset = 0;
        self.pr_list_loading = true;
        self.pr_list_has_more = false;

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
    fn load_more_prs(&mut self) {
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

    /// PR選択時の処理
    ///
    /// L1キャッシュを確認し、Hit/Stale時はキャッシュデータで即座にUI表示しつつ
    /// バックグラウンドで更新チェック/再取得を行う。
    fn select_pr(&mut self, pr_number: u32) {
        self.pr_number = Some(pr_number);
        self.state = AppState::FileList;

        // PR遷移時にバックグラウンドキャッシュをクリア（staleキャッシュ防止）
        self.diff_cache_receiver = None;
        self.prefetch_receiver = None;
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

    /// FileListからPR一覧に戻る
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
            self.comment_submitting = false;
            self.comments_loading = false;
            self.discussion_comments_loading = false;
            self.highlighted_cache_store.clear();
            self.selected_file = 0;
            self.file_list_scroll_offset = 0;
            self.selected_line = 0;
            self.scroll_offset = 0;

            self.state = AppState::PullRequestList;
        }
    }

    /// Create a minimal App instance for unit tests outside of app.rs.
    #[cfg(test)]
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
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
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
            original_pr_number: None,
            saved_pr_snapshot: None,
            saved_local_snapshot: None,
            watcher_handle: None,
            refresh_pending: None,
            markdown_rich: false,
        }
    }

    /// Set the comment_submitting flag for testing.
    #[cfg(test)]
    pub fn set_submitting_for_test(&mut self, submitting: bool) {
        self.comment_submitting = submitting;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_diff_line_index_basic() {
        let patch = r#"@@ -1,3 +1,4 @@
 context line
+added line
 another context
-removed line"#;

        // Line 1 (context) is at diff index 1
        assert_eq!(App::find_diff_line_index(patch, 1), Some(1));
        // Line 2 (added) is at diff index 2
        assert_eq!(App::find_diff_line_index(patch, 2), Some(2));
        // Line 3 (context) is at diff index 3
        assert_eq!(App::find_diff_line_index(patch, 3), Some(3));
        // Line 5 doesn't exist in new file
        assert_eq!(App::find_diff_line_index(patch, 5), None);
    }

    #[test]
    fn test_find_diff_line_index_multi_hunk() {
        let patch = r#"@@ -1,2 +1,2 @@
 line1
+new line2
@@ -10,2 +10,2 @@
 line10
+new line11"#;

        // First hunk: line 1 at index 1, line 2 at index 2
        assert_eq!(App::find_diff_line_index(patch, 1), Some(1));
        assert_eq!(App::find_diff_line_index(patch, 2), Some(2));
        // Second hunk: line 10 at index 4, line 11 at index 5
        assert_eq!(App::find_diff_line_index(patch, 10), Some(4));
        assert_eq!(App::find_diff_line_index(patch, 11), Some(5));
    }

    #[test]
    fn test_has_comment_at_current_line() {
        let config = Config::default();
        let (mut app, _) = App::new_loading("owner/repo", 1, config);
        app.file_comment_positions = vec![
            CommentPosition {
                diff_line_index: 5,
                comment_index: 0,
            },
            CommentPosition {
                diff_line_index: 10,
                comment_index: 1,
            },
        ];

        app.selected_line = 5;
        assert!(app.has_comment_at_current_line());

        app.selected_line = 10;
        assert!(app.has_comment_at_current_line());

        app.selected_line = 7;
        assert!(!app.has_comment_at_current_line());
    }

    #[test]
    fn test_get_comment_indices_at_current_line() {
        let config = Config::default();
        let (mut app, _) = App::new_loading("owner/repo", 1, config);
        // Two comments on line 5, one on line 10
        app.file_comment_positions = vec![
            CommentPosition {
                diff_line_index: 5,
                comment_index: 0,
            },
            CommentPosition {
                diff_line_index: 5,
                comment_index: 2,
            },
            CommentPosition {
                diff_line_index: 10,
                comment_index: 1,
            },
        ];

        app.selected_line = 5;
        let indices = app.get_comment_indices_at_current_line();
        assert_eq!(indices, vec![0, 2]);

        app.selected_line = 10;
        let indices = app.get_comment_indices_at_current_line();
        assert_eq!(indices, vec![1]);

        app.selected_line = 7;
        let indices = app.get_comment_indices_at_current_line();
        assert!(indices.is_empty());
    }

    #[test]
    fn test_jump_to_next_comment_basic() {
        let config = Config::default();
        let (mut app, _) = App::new_loading("owner/repo", 1, config);
        app.file_comment_positions = vec![
            CommentPosition {
                diff_line_index: 5,
                comment_index: 0,
            },
            CommentPosition {
                diff_line_index: 10,
                comment_index: 1,
            },
            CommentPosition {
                diff_line_index: 15,
                comment_index: 2,
            },
        ];

        app.selected_line = 0;
        app.jump_to_next_comment();
        assert_eq!(app.selected_line, 5);

        app.jump_to_next_comment();
        assert_eq!(app.selected_line, 10);

        app.jump_to_next_comment();
        assert_eq!(app.selected_line, 15);
    }

    #[test]
    fn test_jump_to_next_comment_no_wrap() {
        let config = Config::default();
        let (mut app, _) = App::new_loading("owner/repo", 1, config);
        app.file_comment_positions = vec![CommentPosition {
            diff_line_index: 5,
            comment_index: 0,
        }];

        app.selected_line = 5;
        app.jump_to_next_comment();
        // Should stay at 5 (no wrap-around)
        assert_eq!(app.selected_line, 5);
    }

    #[test]
    fn test_jump_to_prev_comment_basic() {
        let config = Config::default();
        let (mut app, _) = App::new_loading("owner/repo", 1, config);
        app.file_comment_positions = vec![
            CommentPosition {
                diff_line_index: 5,
                comment_index: 0,
            },
            CommentPosition {
                diff_line_index: 10,
                comment_index: 1,
            },
            CommentPosition {
                diff_line_index: 15,
                comment_index: 2,
            },
        ];

        app.selected_line = 20;
        app.jump_to_prev_comment();
        assert_eq!(app.selected_line, 15);

        app.jump_to_prev_comment();
        assert_eq!(app.selected_line, 10);

        app.jump_to_prev_comment();
        assert_eq!(app.selected_line, 5);
    }

    #[test]
    fn test_jump_to_prev_comment_no_wrap() {
        let config = Config::default();
        let (mut app, _) = App::new_loading("owner/repo", 1, config);
        app.file_comment_positions = vec![CommentPosition {
            diff_line_index: 5,
            comment_index: 0,
        }];

        app.selected_line = 5;
        app.jump_to_prev_comment();
        // Should stay at 5 (no wrap-around)
        assert_eq!(app.selected_line, 5);
    }

    #[test]
    fn test_jump_with_empty_positions() {
        let config = Config::default();
        let (mut app, _) = App::new_loading("owner/repo", 1, config);
        app.file_comment_positions = vec![];

        app.selected_line = 10;
        app.jump_to_next_comment();
        assert_eq!(app.selected_line, 10);

        app.jump_to_prev_comment();
        assert_eq!(app.selected_line, 10);
    }

    #[test]
    fn test_liststate_autoscroll_with_multiline_items() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::text::Line;
        use ratatui::widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget};

        // 10 multiline items (each 3 lines), area height = 12 (10 inner after borders)
        let items: Vec<ListItem> = (0..10)
            .map(|i| {
                ListItem::new(vec![
                    Line::from(format!("Header {}", i)),
                    Line::from(format!("  Body {}", i)),
                    Line::from(""),
                ])
            })
            .collect();

        let area = Rect::new(0, 0, 40, 12); // 12 total, 10 inner

        // Simulate frame-by-frame scrolling like the real app
        let mut offset = 0usize;
        for selected in 0..10 {
            let list = List::new(items.clone()).block(Block::default().borders(Borders::ALL));
            let mut state = ListState::default()
                .with_offset(offset)
                .with_selected(Some(selected));
            let mut buf = Buffer::empty(area);
            StatefulWidget::render(&list, area, &mut buf, &mut state);
            offset = state.offset();

            // selected should always be in visible range [offset, offset + visible_items)
            // With 10 inner height and 3 lines per item, 3 items fit (9 lines)
            assert!(
                selected >= offset,
                "selected={} should be >= offset={}",
                selected,
                offset
            );
        }

        // After scrolling to last item, offset should be > 0
        assert!(offset > 0, "offset should have scrolled, got {}", offset);
    }

    #[test]
    fn test_back_to_pr_list_clears_view_receivers() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
        app.started_from_pr_list = true;

        // data_receiver is already set by new_loading
        assert!(app.data_receiver.is_some());

        // Set up additional receivers to simulate in-flight requests
        let (_comment_tx, comment_rx) = mpsc::channel(1);
        app.comment_receiver = Some((1, comment_rx));
        let (_disc_tx, disc_rx) = mpsc::channel(1);
        app.discussion_comment_receiver = Some((1, disc_rx));
        let (_submit_tx, submit_rx) = mpsc::channel(1);
        app.comment_submit_receiver = Some((1, submit_rx));
        app.comment_submitting = true;
        app.comments_loading = true;
        app.discussion_comments_loading = true;
        let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(1);
        app.retry_sender = Some(retry_tx);

        app.back_to_pr_list();

        // data_receiver / retry_sender は永続のため維持
        assert!(app.data_receiver.is_some());
        assert!(app.retry_sender.is_some());
        // view 系 receivers はクリア
        assert!(app.comment_receiver.is_none());
        assert!(app.discussion_comment_receiver.is_none());
        assert!(app.comment_submit_receiver.is_none());
        assert!(app.diff_cache_receiver.is_none());
        assert!(app.prefetch_receiver.is_none());
        // Loading flags should be cleared
        assert!(!app.comment_submitting);
        assert!(!app.comments_loading);
        assert!(!app.discussion_comments_loading);
        // PR number should be None
        assert!(app.pr_number.is_none());
        assert_eq!(app.state, AppState::PullRequestList);
    }

    #[test]
    fn test_back_to_pr_list_from_local_mode_resets_local_state() {
        let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
        let (_data_tx, data_rx) = mpsc::channel(2);
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 0, config);
        app.started_from_pr_list = true;
        app.local_mode = true;
        app.pr_number = Some(0);
        app.retry_sender = Some(retry_tx);
        app.data_receiver = Some((0, data_rx));
        app.selected_file = 2;

        app.back_to_pr_list();

        // local_mode がリセットされている
        assert!(!app.local_mode);
        // Local スナップショットが保存されている
        assert!(app.saved_local_snapshot.is_some());
        assert_eq!(app.state, AppState::PullRequestList);
        assert!(app.pr_number.is_none());
    }

    #[tokio::test]
    async fn test_pr_list_local_toggle_round_trip() {
        // PR一覧 → L(Local) → q(PR一覧) → L(Local) の往復でデータが正常に表示されるか
        let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(8);
        let (_data_tx, data_rx) = mpsc::channel(2);
        let mut app = App::new_for_test();
        app.started_from_pr_list = true;
        app.state = AppState::PullRequestList;
        app.pr_number = None;
        app.original_pr_number = None;
        app.retry_sender = Some(retry_tx);
        app.data_receiver = Some((0, data_rx));

        // SessionCache に Local diff データを事前格納
        let local_pr = PullRequest {
            number: 0,
            title: "Local HEAD diff".to_string(),
            body: None,
            state: "local".to_string(),
            head: crate::github::Branch {
                ref_name: "HEAD".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "local".to_string(),
                sha: "local".to_string(),
            },
            user: crate::github::User {
                login: "local".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let local_files = vec![ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1,1 +1,2 @@\n line1\n+line2".to_string()),
        }];
        app.session_cache.put_pr_data(
            PrCacheKey {
                repo: "test/repo".to_string(),
                pr_number: 0,
            },
            PrData {
                pr: Box::new(local_pr),
                files: local_files,
                pr_updated_at: "2024-01-01T00:00:00Z".to_string(),
            },
        );

        // Step 1: PR一覧 → L (Local モード)
        app.toggle_local_mode();
        assert!(app.local_mode);
        assert_eq!(app.pr_number, Some(0));
        assert_eq!(app.state, AppState::FileList);
        assert!(matches!(app.data_state, DataState::Loaded { .. }));

        // Step 2: q → PR一覧に戻る
        app.back_to_pr_list();
        assert!(!app.local_mode);
        assert_eq!(app.state, AppState::PullRequestList);
        assert!(app.saved_local_snapshot.is_some());

        // Step 3: L → 再度 Local モード（1回目で正しく Local に入る）
        app.toggle_local_mode();
        assert!(app.local_mode);
        assert_eq!(app.pr_number, Some(0));
        assert_eq!(app.state, AppState::FileList);
        // SessionCache から即時表示
        assert!(matches!(app.data_state, DataState::Loaded { .. }));
    }

    #[tokio::test]
    async fn test_poll_data_updates_discards_stale_pr_data() {
        let config = Config::default();
        let (mut app, tx) = App::new_loading("owner/repo", 1, config);
        app.started_from_pr_list = true;

        // Simulate switching to PR #2 while PR #1 data is in-flight
        // The data_receiver still carries origin_pr = 1
        app.pr_number = Some(2);

        // Send data for PR #1
        let pr = PullRequest {
            number: 1,
            title: "PR 1".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };
        tx.send(DataLoadResult::Success {
            pr: Box::new(pr),
            files: vec![],
        })
        .await
        .unwrap();

        // Poll should NOT panic and should NOT apply PR #1 data to current UI state
        app.poll_data_updates();

        // data_receiver should be kept alive (persistent channel for future refreshes)
        assert!(app.data_receiver.is_some());
        // data_state should still be Loading (PR #1 data was discarded from UI)
        assert!(matches!(app.data_state, DataState::Loading));
        // But session cache should have the data under PR #1 key
        let cache_key = PrCacheKey {
            repo: "owner/repo".to_string(),
            pr_number: 1,
        };
        assert!(app.session_cache.get_pr_data(&cache_key).is_some());
    }

    #[tokio::test]
    async fn test_poll_comment_updates_discards_stale_pr_comments() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
        app.started_from_pr_list = true;

        // Set up a comment receiver for PR #1
        let (comment_tx, comment_rx) = mpsc::channel(1);
        app.comment_receiver = Some((1, comment_rx));
        app.comments_loading = true;

        // Simulate switching to PR #2
        app.pr_number = Some(2);

        // Send comments for PR #1
        comment_tx.send(Ok(vec![])).await.unwrap();

        // Poll should NOT panic and should NOT apply PR #1 comments to UI
        app.poll_comment_updates();

        assert!(app.comment_receiver.is_none());
        // comments_loading should NOT have been cleared (different PR)
        assert!(app.comments_loading);
        // Session cache should NOT have comments for PR #1 since pr_data was never stored
        // (comments are only cached for keys that have an existing pr_data entry)
        let cache_key = PrCacheKey {
            repo: "owner/repo".to_string(),
            pr_number: 1,
        };
        assert!(app.session_cache.get_review_comments(&cache_key).is_none());
    }

    #[tokio::test]
    async fn test_handle_data_result_clamps_selected_file_when_files_shrink() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

        // Simulate initial state with 5 files, selected_file pointing to file index 4
        let make_file = |name: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        };

        let initial_files: Vec<ChangedFile> = (0..5)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        // Set initial loaded state with 5 files
        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files,
        };
        app.selected_file = 4; // Last file selected

        // Now simulate refresh with only 2 files (file count shrank)
        let fewer_files: Vec<ChangedFile> = (0..2)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: fewer_files,
            },
        );

        // selected_file should be clamped to 1 (last valid index for 2 files)
        assert_eq!(app.selected_file, 1);
        // Should be able to access the file without panic
        assert!(app.files().get(app.selected_file).is_some());
    }

    #[tokio::test]
    async fn test_handle_data_result_resyncs_diff_state_when_selected_file_changes() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

        let make_file = |name: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        };

        let initial_files: Vec<ChangedFile> = (0..5)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        // Set initial loaded state with 5 files
        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files,
        };
        app.selected_file = 4;
        app.selected_line = 10;
        app.scroll_offset = 5;

        // Set a stale diff_cache pointing to old file index 4
        app.diff_cache = Some(DiffCache {
            file_index: 4,
            patch_hash: 0,
            lines: vec![],
            interner: Rodeo::default(),
            highlighted: false,
            markdown_rich: false,
        });

        // Refresh with only 2 files (selected_file will be clamped from 4 to 1)
        let fewer_files: Vec<ChangedFile> = (0..2)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: fewer_files,
            },
        );

        // selected_file clamped
        assert_eq!(app.selected_file, 1);
        // diff_cache must be rebuilt for the new selected file (ensure_diff_cache rebuilds it)
        assert_eq!(
            app.diff_cache.as_ref().map(|c| c.file_index),
            Some(1),
            "diff_cache should be rebuilt for the new selected file"
        );
        // selected_line and scroll_offset must be reset
        assert_eq!(app.selected_line, 0, "selected_line should be reset to 0");
        assert_eq!(app.scroll_offset, 0, "scroll_offset should be reset to 0");
    }

    #[tokio::test]
    async fn test_handle_data_result_resyncs_comment_positions_when_selected_file_changes() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

        let make_file = |name: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        };

        let initial_files: Vec<ChangedFile> = (0..5)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        // Set initial loaded state with 5 files, selected_file = 4
        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files,
        };
        app.selected_file = 4;

        // Set up review comments for file_4.rs (the old selected file)
        app.review_comments = Some(vec![ReviewComment {
            id: 1,
            path: "file_4.rs".to_string(),
            line: Some(1),
            body: "comment on old file".to_string(),
            user: crate::github::User {
                login: "reviewer".to_string(),
            },
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }]);

        // Pre-populate stale comment positions for the old file
        app.file_comment_positions = vec![CommentPosition {
            diff_line_index: 2,
            comment_index: 0,
        }];
        app.file_comment_lines.insert(2);
        app.comment_panel_open = true;
        app.comment_panel_scroll = 5;

        // Refresh with only 2 files (selected_file will be clamped from 4 to 1)
        let fewer_files: Vec<ChangedFile> = (0..2)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: fewer_files,
            },
        );

        // selected_file clamped to 1
        assert_eq!(app.selected_file, 1);

        // file_comment_positions should be recalculated for file_1.rs (no matching comments)
        assert!(
            app.file_comment_positions.is_empty(),
            "file_comment_positions should be recalculated for new file (no comments for file_1.rs)"
        );
        assert!(
            app.file_comment_lines.is_empty(),
            "file_comment_lines should be recalculated for new file"
        );

        // comment_panel should be closed
        assert!(
            !app.comment_panel_open,
            "comment_panel_open should be reset when selected_file changes"
        );
        assert_eq!(
            app.comment_panel_scroll, 0,
            "comment_panel_scroll should be reset when selected_file changes"
        );
    }

    #[tokio::test]
    async fn test_handle_data_result_preserves_diff_state_when_selected_file_unchanged() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

        let make_file = |name: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        };

        let initial_files: Vec<ChangedFile> = (0..5)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        // Set initial loaded state
        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files,
        };
        app.selected_file = 1;
        app.selected_line = 10;
        app.scroll_offset = 5;

        // Set diff_cache for file index 1
        app.diff_cache = Some(DiffCache {
            file_index: 1,
            patch_hash: 0,
            lines: vec![],
            interner: Rodeo::default(),
            highlighted: false,
            markdown_rich: false,
        });

        // Refresh with same or more files (selected_file stays at 1)
        let same_files: Vec<ChangedFile> = (0..5)
            .map(|i| make_file(&format!("file_{}.rs", i)))
            .collect();

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: same_files,
            },
        );

        // selected_file unchanged
        assert_eq!(app.selected_file, 1);
        // diff_cache should NOT be invalidated (selected_file didn't change)
        assert!(
            app.diff_cache.is_some(),
            "diff_cache should be preserved when selected_file is unchanged"
        );
        // selected_line and scroll_offset should be preserved
        assert_eq!(app.selected_line, 10);
        assert_eq!(app.scroll_offset, 5);
    }

    #[tokio::test]
    async fn test_handle_data_result_keeps_selected_file_by_filename() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
        app.set_local_mode(true);
        app.set_local_auto_focus(false);

        let make_file = |name: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        };

        let initial_files: Vec<ChangedFile> = vec![
            make_file("file_a.rs"),
            make_file("file_b.rs"),
            make_file("file_c.rs"),
        ];

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files.clone(),
        };
        app.selected_file = 1; // file_b.rs
        app.remember_local_file_signatures(&initial_files);

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: vec![make_file("file_b.rs"), make_file("file_c.rs")],
            },
        );

        assert_eq!(
            app.selected_file, 0,
            "selected file should track file_b.rs by filename, not by index"
        );
    }

    #[tokio::test]
    async fn test_handle_data_result_auto_focus_selects_next_changed_file() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
        app.set_local_mode(true);
        app.set_local_auto_focus(true);
        app.selected_file = 1;

        let make_file = |name: &str, patch: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some(patch.to_string()),
        };

        let initial_files = vec![
            make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_c.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_d.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
        ];

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files.clone(),
        };
        app.remember_local_file_signatures(&initial_files);

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: vec![
                    make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new2"),
                    make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
                    make_file("file_c.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
                    make_file("file_d.rs", "@@ -1,1 +1,1 @@\n-old\n+new2"),
                ],
            },
        );

        assert_eq!(
            app.selected_file, 3,
            "auto-focus should prefer the next changed file after current selection"
        );
    }

    #[tokio::test]
    async fn test_handle_data_result_auto_focus_prefers_nearest_changed_file() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
        app.set_local_mode(true);
        app.set_local_auto_focus(true);
        app.selected_file = 3;

        let make_file = |name: &str, patch: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some(patch.to_string()),
        };

        let initial_files = vec![
            make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_c.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_d.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_e.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
        ];

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files.clone(),
        };
        app.remember_local_file_signatures(&initial_files);

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: vec![
                    make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new2"), // changed before
                    make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),  // unchanged
                    make_file("file_c.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),  // unchanged
                    make_file("file_d.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),  // unchanged
                    make_file("file_e.rs", "@@ -1,1 +1,1 @@\n-old\n+new2"), // changed after
                ],
            },
        );

        assert_eq!(
            app.selected_file, 4,
            "auto-focus should move to the nearer changed file around current selection"
        );
    }

    #[tokio::test]
    async fn test_handle_data_result_auto_focus_prefers_next_when_distances_are_tie() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
        app.set_local_mode(true);
        app.set_local_auto_focus(true);
        app.selected_file = 2;

        let make_file = |name: &str, patch: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some(patch.to_string()),
        };

        let initial_files = vec![
            make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_c.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_d.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
            make_file("file_e.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
        ];

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        app.data_state = DataState::Loaded {
            pr: pr.clone(),
            files: initial_files.clone(),
        };
        app.remember_local_file_signatures(&initial_files);

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr,
                files: vec![
                    make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new"), // unchanged (index 0)
                    make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-old\n+new2"), // changed (index 1)
                    make_file("file_c.rs", "@@ -1,1 +1,1 @@\n-old\n+new"), // unchanged (index 2)
                    make_file("file_d.rs", "@@ -1,1 +1,1 @@\n-old\n+new2"), // changed (index 3)
                    make_file("file_e.rs", "@@ -1,1 +1,1 @@\n-old\n+new"), // unchanged (index 4)
                ],
            },
        );

        assert_eq!(
            app.selected_file, 3,
            "auto-focus should prefer the next file when before/after distances are equal"
        );
    }

    #[tokio::test]
    async fn test_handle_data_result_auto_focus_transitions_to_split_view_diff() {
        let config = Config::default();
        let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
        app.set_local_mode(true);
        app.set_local_auto_focus(true);
        app.state = AppState::FileList;

        let make_file = |name: &str, patch: &str| ChangedFile {
            filename: name.to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some(patch.to_string()),
        };

        let pr = Box::new(PullRequest {
            number: 1,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        });

        app.handle_data_result(
            1,
            DataLoadResult::Success {
                pr: pr.clone(),
                files: vec![make_file("initial.rs", "@@ -1,1 +1,1 @@\n-old\n+new")],
            },
        );

        assert_eq!(app.state, AppState::SplitViewDiff);
        assert_eq!(app.selected_file, 0);
        assert_eq!(app.files().len(), 1);
    }

    #[test]
    fn test_toggle_auto_focus() {
        let mut app = App::new_for_test();
        app.local_mode = true;
        assert!(!app.local_auto_focus);

        app.toggle_auto_focus();
        assert!(app.local_auto_focus);
        assert!(app.submission_result.is_some());
        assert!(app.submission_result.as_ref().unwrap().1.contains("ON"));

        app.toggle_auto_focus();
        assert!(!app.local_auto_focus);
        assert!(app.submission_result.as_ref().unwrap().1.contains("OFF"));
    }

    #[test]
    fn test_toggle_local_mode_blocks_during_ai_rally() {
        let mut app = App::new_for_test();
        app.state = AppState::AiRally;

        app.toggle_local_mode();
        assert!(!app.local_mode);
        assert!(app.submission_result.as_ref().unwrap().1.contains("Cannot"));
    }

    #[test]
    fn test_save_and_restore_view_snapshot() {
        let mut app = App::new_for_test();
        app.selected_file = 5;
        app.file_list_scroll_offset = 2;
        app.selected_line = 10;
        app.scroll_offset = 3;

        let snapshot = app.save_view_snapshot();

        // save_view_snapshot does not move data_state (ViewSnapshot has no data_state)
        // App state fields should be reset after save
        assert!(app.diff_cache.is_none());

        // Modify app state
        app.selected_file = 0;
        app.selected_line = 0;

        // Restore
        app.restore_view_snapshot(snapshot);
        assert_eq!(app.selected_file, 5);
        assert_eq!(app.file_list_scroll_offset, 2);
        assert_eq!(app.selected_line, 10);
        assert_eq!(app.scroll_offset, 3);
    }

    #[test]
    fn test_toggle_local_mode_pr_to_local_and_back() {
        let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
        let (_data_tx, data_rx) = mpsc::channel(2);
        let mut app = App::new_for_test();
        app.retry_sender = Some(retry_tx);
        app.data_receiver = Some((42, data_rx));
        app.original_pr_number = Some(42);
        app.pr_number = Some(42);
        app.selected_file = 3;

        // PR → Local
        app.toggle_local_mode();
        assert!(app.local_mode);
        assert_eq!(app.pr_number, Some(0));
        assert!(app.saved_pr_snapshot.is_some());
        assert!(app.submission_result.as_ref().unwrap().1.contains("Local"));

        // Local → PR
        app.toggle_local_mode();
        assert!(!app.local_mode);
        assert!(app.saved_local_snapshot.is_some());
        // saved_pr_snapshot が復元されたので取得済み
        assert!(app.saved_pr_snapshot.is_none());
        assert_eq!(app.selected_file, 3); // 復元された値
        assert!(app.submission_result.as_ref().unwrap().1.contains("PR"));
    }

    #[test]
    fn test_toggle_local_mode_no_pr_to_return() {
        let mut app = App::new_for_test();
        app.original_pr_number = None;
        app.started_from_pr_list = false;
        app.local_mode = true;

        // Local → PR: 復帰先がない
        app.toggle_local_mode();
        // local_mode のまま（エラートースト）
        assert!(app.local_mode);
        assert!(app.submission_result.as_ref().unwrap().1.contains("No PR"));
    }

    #[test]
    fn test_retry_load_sends_correct_request_type() {
        let (tx, mut rx) = mpsc::channel::<RefreshRequest>(1);
        let mut app = App::new_for_test();
        app.retry_sender = Some(tx);

        // PR mode
        app.local_mode = false;
        app.pr_number = Some(42);
        app.retry_load();
        let req = rx.try_recv().unwrap();
        assert!(matches!(req, RefreshRequest::PrRefresh { pr_number: 42 }));

        // Local mode
        app.local_mode = true;
        app.data_state = DataState::Loading; // reset from retry_load
        app.retry_load();
        let req = rx.try_recv().unwrap();
        assert!(matches!(req, RefreshRequest::LocalRefresh));
    }

    #[tokio::test]
    async fn test_handle_data_result_auto_focus_skips_state_transition_during_bg_rally() {
        let mut app = App::new_for_test();
        app.local_mode = true;
        app.local_auto_focus = true;
        app.state = AppState::FileList;

        // Set up BG rally state (active but not in AiRally AppState)
        app.ai_rally_state = Some(AiRallyState {
            iteration: 1,
            max_iterations: 10,
            state: crate::ai::RallyState::ReviewerReviewing,
            history: vec![],
            logs: vec![],
            log_scroll_offset: 0,
            selected_log_index: None,
            showing_log_detail: false,
            pending_question: None,
            pending_permission: None,
            pending_review_post: None,
            pending_fix_post: None,
            last_visible_log_height: 0,
        });

        let pr = Box::new(make_local_pr());
        let files = vec![ChangedFile {
            filename: "new.rs".to_string(),
            status: "added".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -0,0 +1,1 @@\n+new content".to_string()),
        }];

        app.handle_data_result(0, DataLoadResult::Success { pr, files });

        // State should NOT transition to SplitViewDiff during BG rally
        assert_eq!(app.state, AppState::FileList);
        // But file selection IS updated
        assert_eq!(app.selected_file, 0);
    }

    fn make_local_pr() -> PullRequest {
        PullRequest {
            number: 0,
            title: "Local diff".to_string(),
            body: None,
            state: "local".to_string(),
            base: crate::github::Branch {
                ref_name: "local".to_string(),
                sha: "".to_string(),
            },
            head: crate::github::Branch {
                ref_name: "HEAD".to_string(),
                sha: "".to_string(),
            },
            user: crate::github::User {
                login: "local".to_string(),
            },
            updated_at: "".to_string(),
        }
    }

    #[test]
    fn test_toggle_markdown_rich() {
        let mut app = App::new_for_test();
        // Set up loaded state with a markdown file
        app.data_state = DataState::Loaded {
            pr: Box::new(make_local_pr()),
            files: vec![ChangedFile {
                filename: "README.md".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+test".to_string()),
            }],
        };

        assert!(!app.is_markdown_rich());

        app.toggle_markdown_rich();
        assert!(app.is_markdown_rich());
        assert!(
            app.diff_cache.is_none(),
            "Cache should be cleared for md file"
        );

        app.toggle_markdown_rich();
        assert!(!app.is_markdown_rich());
    }

    #[test]
    fn test_toggle_markdown_rich_clears_receivers() {
        let mut app = App::new_for_test();
        // Set up loaded state with a markdown file
        app.data_state = DataState::Loaded {
            pr: Box::new(make_local_pr()),
            files: vec![ChangedFile {
                filename: "README.md".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+test".to_string()),
            }],
        };

        // Simulate having active receivers
        let (_tx, rx) = tokio::sync::mpsc::channel::<DiffCache>(1);
        app.diff_cache_receiver = Some(rx);

        let (_tx2, rx2) = tokio::sync::mpsc::channel::<DiffCache>(1);
        app.prefetch_receiver = Some(rx2);

        app.toggle_markdown_rich();
        assert!(
            app.diff_cache_receiver.is_none(),
            "diff_cache_receiver should be cleared for md file"
        );
        assert!(
            app.prefetch_receiver.is_none(),
            "prefetch_receiver should be cleared on toggle"
        );
    }

    #[test]
    fn test_toggle_markdown_rich_clears_only_md_cache() {
        let mut app = App::new_for_test();
        // Set up loaded state with both md and non-md files
        app.data_state = DataState::Loaded {
            pr: Box::new(make_local_pr()),
            files: vec![
                ChangedFile {
                    filename: "README.md".to_string(),
                    status: "modified".to_string(),
                    additions: 1,
                    deletions: 0,
                    patch: Some("@@ -1 +1 @@\n+test".to_string()),
                },
                ChangedFile {
                    filename: "main.rs".to_string(),
                    status: "modified".to_string(),
                    additions: 1,
                    deletions: 0,
                    patch: Some("@@ -1 +1 @@\n+fn main(){}".to_string()),
                },
            ],
        };

        // Add cache entries for both files
        let md_cache = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+test");
        let mut rs_cache =
            crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+fn main(){}");
        rs_cache.file_index = 1;
        app.highlighted_cache_store.insert(0, md_cache);
        app.highlighted_cache_store.insert(1, rs_cache);
        assert_eq!(app.highlighted_cache_store.len(), 2);

        app.toggle_markdown_rich();

        // Only md cache should be removed
        assert!(
            !app.highlighted_cache_store.contains_key(&0),
            "md cache should be cleared"
        );
        assert!(
            app.highlighted_cache_store.contains_key(&1),
            "rs cache should be preserved"
        );
        assert_eq!(app.highlighted_cache_store.len(), 1);
    }

    #[test]
    fn test_toggle_markdown_rich_preserves_non_md_diff_cache() {
        let mut app = App::new_for_test();
        // Current file is non-markdown
        app.data_state = DataState::Loaded {
            pr: Box::new(make_local_pr()),
            files: vec![ChangedFile {
                filename: "main.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+fn main(){}".to_string()),
            }],
        };

        let rs_cache =
            crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+fn main(){}");
        app.diff_cache = Some(rs_cache);

        app.toggle_markdown_rich();

        assert!(
            app.diff_cache.is_some(),
            "non-md diff_cache should be preserved"
        );
    }
}
