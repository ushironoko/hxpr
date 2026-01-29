use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, text::Span, Terminal};
use std::io::Stdout;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::ai::orchestrator::{OrchestratorCommand, RallyEvent};
use crate::ai::{Context, Orchestrator, RallyState};
use crate::config::Config;
use crate::github::comment::{DiscussionComment, ReviewComment};
use crate::github::{self, ChangedFile, PullRequest};
use crate::loader::{CommentSubmitResult, DataLoadResult};
use crate::ui;
use crate::ui::text_area::{TextArea, TextAreaAction};
use std::time::Instant;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

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

/// Diff行のキャッシュ（シンタックスハイライト済み）
#[derive(Clone)]
pub struct CachedDiffLine {
    /// 基本の Span（REVERSED なし）
    pub spans: Vec<Span<'static>>,
}

/// Diff表示のキャッシュ
pub struct DiffCache {
    /// キャッシュ対象のファイルインデックス
    pub file_index: usize,
    /// patch のハッシュ（変更検出用）
    pub patch_hash: u64,
    /// コメント行のセット（キャッシュ無効化判定用）
    pub comment_lines: HashSet<usize>,
    /// パース済みの行データ
    pub lines: Vec<CachedDiffLine>,
}

/// 文字列のハッシュを計算
fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// 行ベース入力のコンテキスト（コメント/サジェスチョン共通）
#[derive(Debug, Clone)]
pub struct LineInputContext {
    pub file_index: usize,
    pub line_number: u32,
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
    FileList,
    DiffView,
    TextInput,
    CommentList,
    Help,
    AiRally,
    SplitViewFileList,
    SplitViewDiff,
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
    /// Last rendered visible log height (updated by UI render)
    pub last_visible_log_height: usize,
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
    pub pr_number: u32,
    pub data_state: DataState,
    pub state: AppState,
    /// DiffView で q/Esc を押した時の戻り先
    pub diff_view_return_state: AppState,
    /// CommentPreview/SuggestionPreview の戻り先
    pub preview_return_state: AppState,
    /// Help/CommentList など汎用的な戻り先
    pub previous_state: AppState,
    pub selected_file: usize,
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
    data_receiver: Option<mpsc::Receiver<DataLoadResult>>,
    retry_sender: Option<mpsc::Sender<()>>,
    comment_receiver: Option<mpsc::Receiver<Result<Vec<ReviewComment>, String>>>,
    discussion_comment_receiver: Option<mpsc::Receiver<Result<Vec<DiscussionComment>, String>>>,
    rally_event_receiver: Option<mpsc::Receiver<RallyEvent>>,
    // Handle for aborting the rally orchestrator task
    rally_abort_handle: Option<AbortHandle>,
    // Command sender to communicate with the orchestrator
    rally_command_sender: Option<mpsc::Sender<OrchestratorCommand>>,
    // Flag to start AI Rally when data is loaded (set by --ai-rally CLI flag)
    start_ai_rally_on_load: bool,
    // Comment submission state
    comment_submit_receiver: Option<mpsc::Receiver<CommentSubmitResult>>,
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
    /// 'g' キー入力待ち状態（gd, gg などの2キーコマンド用）
    pub pending_g_key: bool,
    /// シンボル選択ポップアップの状態
    pub symbol_popup: Option<SymbolPopupState>,
}

impl App {
    /// Loading状態で開始（キャッシュミス時）
    pub fn new_loading(
        repo: &str,
        pr_number: u32,
        config: Config,
    ) -> (Self, mpsc::Sender<DataLoadResult>) {
        let (tx, rx) = mpsc::channel(2);

        let app = Self {
            repo: repo.to_string(),
            pr_number,
            data_state: DataState::Loading,
            state: AppState::FileList,
            diff_view_return_state: AppState::FileList,
            preview_return_state: AppState::DiffView,
            previous_state: AppState::FileList,
            selected_file: 0,
            selected_line: 0,
            diff_line_count: 0,
            scroll_offset: 0,
            input_mode: None,
            input_text_area: TextArea::new(),
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
            discussion_comments: None,
            selected_discussion_comment: 0,
            discussion_comment_list_scroll_offset: 0,
            discussion_comments_loading: false,
            discussion_comment_detail_mode: false,
            discussion_comment_detail_scroll: 0,
            comment_tab: CommentTab::default(),
            ai_rally_state: None,
            working_dir: None,
            data_receiver: Some(rx),
            retry_sender: None,
            comment_receiver: None,
            discussion_comment_receiver: None,
            rally_event_receiver: None,
            rally_abort_handle: None,
            rally_command_sender: None,
            start_ai_rally_on_load: false,
            comment_submit_receiver: None,
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
            spinner_frame: 0,
            selected_inline_comment: 0,
            jump_stack: Vec::new(),
            pending_g_key: false,
            symbol_popup: None,
        };

        (app, tx)
    }

    /// キャッシュデータで即座に開始（キャッシュヒット時）
    pub fn new_with_cache(
        repo: &str,
        pr_number: u32,
        config: Config,
        pr: PullRequest,
        files: Vec<ChangedFile>,
    ) -> (Self, mpsc::Sender<DataLoadResult>) {
        let (tx, rx) = mpsc::channel(2);
        let diff_line_count = Self::calc_diff_line_count(&files, 0);

        let app = Self {
            repo: repo.to_string(),
            pr_number,
            data_state: DataState::Loaded {
                pr: Box::new(pr),
                files,
            },
            state: AppState::FileList,
            diff_view_return_state: AppState::FileList,
            preview_return_state: AppState::DiffView,
            previous_state: AppState::FileList,
            selected_file: 0,
            selected_line: 0,
            diff_line_count,
            scroll_offset: 0,
            input_mode: None,
            input_text_area: TextArea::new(),
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
            discussion_comments: None,
            selected_discussion_comment: 0,
            discussion_comment_list_scroll_offset: 0,
            discussion_comments_loading: false,
            discussion_comment_detail_mode: false,
            discussion_comment_detail_scroll: 0,
            comment_tab: CommentTab::default(),
            ai_rally_state: None,
            working_dir: None,
            data_receiver: Some(rx),
            retry_sender: None,
            comment_receiver: None,
            discussion_comment_receiver: None,
            rally_event_receiver: None,
            rally_abort_handle: None,
            rally_command_sender: None,
            start_ai_rally_on_load: false,
            comment_submit_receiver: None,
            comment_submitting: false,
            submission_result: None,
            submission_result_time: None,
            spinner_frame: 0,
            selected_inline_comment: 0,
            jump_stack: Vec::new(),
            pending_g_key: false,
            symbol_popup: None,
        };

        (app, tx)
    }

    pub fn set_retry_sender(&mut self, tx: mpsc::Sender<()>) {
        self.retry_sender = Some(tx);
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = ui::setup_terminal()?;

        // Start AI Rally immediately if flag is set and data is already loaded (from cache)
        if self.start_ai_rally_on_load && matches!(self.data_state, DataState::Loaded { .. }) {
            self.start_ai_rally_on_load = false;
            self.start_ai_rally();
        }

        while !self.should_quit {
            self.spinner_frame = self.spinner_frame.wrapping_add(1);
            self.poll_data_updates();
            self.poll_comment_updates();
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

    /// Set flag to start AI Rally when data is loaded (used by --ai-rally CLI flag)
    pub fn set_start_ai_rally_on_load(&mut self, start: bool) {
        self.start_ai_rally_on_load = start;
    }

    /// バックグラウンドタスクからのデータ更新をポーリング
    fn poll_data_updates(&mut self) {
        let Some(ref mut rx) = self.data_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => self.handle_data_result(result),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.data_receiver = None;
            }
        }
    }

    /// コメント取得のポーリング
    fn poll_comment_updates(&mut self) {
        let Some(ref mut rx) = self.comment_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(comments)) => {
                self.review_comments = Some(comments);
                self.selected_comment = 0;
                self.comment_list_scroll_offset = 0;
                self.comments_loading = false;
                self.comment_receiver = None;
                // Update comment positions if in diff view or side-by-side
                if matches!(
                    self.state,
                    AppState::DiffView | AppState::SplitViewDiff | AppState::SplitViewFileList
                ) {
                    self.update_file_comment_positions();
                    self.ensure_diff_cache();
                }
            }
            Ok(Err(e)) => {
                eprintln!("Warning: Failed to fetch comments: {}", e);
                // Keep existing comments if any, or show empty
                if self.review_comments.is_none() {
                    self.review_comments = Some(vec![]);
                }
                self.comments_loading = false;
                self.comment_receiver = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // Keep existing comments if any, or show empty
                if self.review_comments.is_none() {
                    self.review_comments = Some(vec![]);
                }
                self.comments_loading = false;
                self.comment_receiver = None;
            }
        }
    }

    /// Discussion コメント取得のポーリング
    fn poll_discussion_comment_updates(&mut self) {
        let Some(ref mut rx) = self.discussion_comment_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(Ok(comments)) => {
                self.discussion_comments = Some(comments);
                self.selected_discussion_comment = 0;
                self.discussion_comments_loading = false;
                self.discussion_comment_receiver = None;
            }
            Ok(Err(e)) => {
                eprintln!("Warning: Failed to fetch discussion comments: {}", e);
                if self.discussion_comments.is_none() {
                    self.discussion_comments = Some(vec![]);
                }
                self.discussion_comments_loading = false;
                self.discussion_comment_receiver = None;
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                if self.discussion_comments.is_none() {
                    self.discussion_comments = Some(vec![]);
                }
                self.discussion_comments_loading = false;
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

        let Some(ref mut rx) = self.comment_submit_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(CommentSubmitResult::Success) => {
                self.comment_submitting = false;
                self.comment_submit_receiver = None;
                self.submission_result = Some((true, "Submitted".to_string()));
                self.submission_result_time = Some(Instant::now());
                // ファイルキャッシュを破棄してコメントを再取得
                let _ = crate::cache::invalidate_comment_cache(&self.repo, self.pr_number);
                self.review_comments = None;
                self.load_review_comments();
                self.update_file_comment_positions();
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
                            }
                            RallyEvent::IterationStarted(i) => {
                                rally_state.iteration = *i;
                                rally_state.logs.push(LogEntry::new(
                                    LogEventType::Info,
                                    format!("Starting iteration {}", i),
                                ));
                            }
                            RallyEvent::Log(msg) => {
                                rally_state
                                    .logs
                                    .push(LogEntry::new(LogEventType::Info, msg.clone()));
                            }
                            RallyEvent::AgentThinking(content) => {
                                // Store full content; truncation happens at display time
                                rally_state
                                    .logs
                                    .push(LogEntry::new(LogEventType::Thinking, content.clone()));
                            }
                            RallyEvent::AgentToolUse(tool_name, input) => {
                                rally_state.logs.push(LogEntry::new(
                                    LogEventType::ToolUse,
                                    format!("{}: {}", tool_name, input),
                                ));
                            }
                            RallyEvent::AgentToolResult(tool_name, result) => {
                                // Store full content; truncation happens at display time
                                rally_state.logs.push(LogEntry::new(
                                    LogEventType::ToolResult,
                                    format!("{}: {}", tool_name, result),
                                ));
                            }
                            RallyEvent::AgentText(text) => {
                                // Store full content; truncation happens at display time
                                rally_state
                                    .logs
                                    .push(LogEntry::new(LogEventType::Text, text.clone()));
                            }
                            RallyEvent::ReviewCompleted(_) => {
                                rally_state.logs.push(LogEntry::new(
                                    LogEventType::Review,
                                    "Review completed".to_string(),
                                ));
                            }
                            RallyEvent::FixCompleted(fix) => {
                                rally_state.logs.push(LogEntry::new(
                                    LogEventType::Fix,
                                    format!("Fix completed: {}", fix.summary),
                                ));
                            }
                            RallyEvent::Error(e) => {
                                rally_state
                                    .logs
                                    .push(LogEntry::new(LogEventType::Error, e.clone()));
                            }
                            RallyEvent::ClarificationNeeded(question) => {
                                rally_state.pending_question = Some(question.clone());
                                rally_state.logs.push(LogEntry::new(
                                    LogEventType::Info,
                                    format!("Clarification needed: {}", question),
                                ));
                            }
                            RallyEvent::PermissionNeeded(action, reason) => {
                                rally_state.pending_permission = Some(PermissionInfo {
                                    action: action.clone(),
                                    reason: reason.clone(),
                                });
                                rally_state.logs.push(LogEntry::new(
                                    LogEventType::Info,
                                    format!("Permission needed: {} - {}", action, reason),
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
                            rally_state.logs.push(LogEntry::new(
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

    fn handle_data_result(&mut self, result: DataLoadResult) {
        match result {
            DataLoadResult::Success { pr, files } => {
                self.diff_line_count = Self::calc_diff_line_count(&files, self.selected_file);
                // Check if we need to start AI Rally (--ai-rally flag was passed)
                let should_start_rally =
                    self.start_ai_rally_on_load && matches!(self.data_state, DataState::Loading);
                self.data_state = DataState::Loaded { pr, files };
                if should_start_rally {
                    self.start_ai_rally_on_load = false; // Clear the flag
                    self.start_ai_rally();
                }
            }
            DataLoadResult::Error(msg) => {
                // Loading状態の場合のみエラー表示（既にデータがある場合は無視）
                if matches!(self.data_state, DataState::Loading) {
                    self.data_state = DataState::Error(msg);
                }
            }
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

                match self.state {
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
            self.data_state = DataState::Loading;
            let _ = tx.try_send(());
        }
    }

    async fn handle_file_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.files().is_empty() {
                    self.selected_file =
                        (self.selected_file + 1).min(self.files().len().saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_file = self.selected_file.saturating_sub(1);
            }
            // Split view を開く際は diff ペインにフォーカスした状態で遷移する。
            // ファイル一覧側へのフォーカス切替は ←/h で行う。
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if !self.files().is_empty() {
                    self.state = AppState::SplitViewDiff;
                    self.sync_diff_to_selected_file();
                }
            }
            KeyCode::Char(c) if c == self.config.keybindings.approve => {
                self.submit_review(ReviewAction::Approve, terminal).await?
            }
            KeyCode::Char(c) if c == self.config.keybindings.request_changes => {
                self.submit_review(ReviewAction::RequestChanges, terminal)
                    .await?
            }
            KeyCode::Char(c) if c == self.config.keybindings.comment => {
                self.submit_review(ReviewAction::Comment, terminal).await?
            }
            KeyCode::Char('C') => {
                self.previous_state = AppState::FileList;
                self.open_comment_list();
            }
            KeyCode::Char('R') => self.refresh_all(),
            KeyCode::Char('A') => self.resume_or_start_ai_rally(),
            KeyCode::Char('?') => {
                self.previous_state = AppState::FileList;
                self.state = AppState::Help;
            }
            _ => {}
        }
        Ok(())
    }

    /// FileList 系状態で共通のキーを処理する。処理した場合は true を返す。
    async fn handle_common_file_list_keys(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<bool> {
        match key.code {
            KeyCode::Char(c) if c == self.config.keybindings.approve => {
                self.submit_review(ReviewAction::Approve, terminal).await?;
                Ok(true)
            }
            KeyCode::Char(c) if c == self.config.keybindings.request_changes => {
                self.submit_review(ReviewAction::RequestChanges, terminal)
                    .await?;
                Ok(true)
            }
            KeyCode::Char(c) if c == self.config.keybindings.comment => {
                self.submit_review(ReviewAction::Comment, terminal).await?;
                Ok(true)
            }
            KeyCode::Char('R') => {
                self.refresh_all();
                Ok(true)
            }
            KeyCode::Char('A') => {
                self.resume_or_start_ai_rally();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn handle_split_view_file_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.files().is_empty() {
                    self.selected_file =
                        (self.selected_file + 1).min(self.files().len().saturating_sub(1));
                    self.sync_diff_to_selected_file();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_file > 0 {
                    self.selected_file = self.selected_file.saturating_sub(1);
                    self.sync_diff_to_selected_file();
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if !self.files().is_empty() {
                    self.state = AppState::SplitViewDiff;
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('q') | KeyCode::Esc => {
                self.state = AppState::FileList;
            }
            KeyCode::Char('C') => {
                self.previous_state = AppState::SplitViewFileList;
                self.open_comment_list();
            }
            KeyCode::Char('?') => {
                self.previous_state = AppState::SplitViewFileList;
                self.state = AppState::Help;
            }
            _ => {
                self.handle_common_file_list_keys(key, terminal).await?;
            }
        }
        Ok(())
    }

    async fn handle_split_view_diff_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // シンボルポップアップ表示中
        if self.symbol_popup.is_some() {
            self.handle_symbol_popup_input(key, terminal).await?;
            return Ok(());
        }

        // 右ペインの実高さを計算（split view レイアウトと同じロジック）
        let term_size = terminal.size()?;
        let term_height = term_size.height as usize;
        let term_width = term_size.width as usize;
        // Header(3) + Footer(3) + border(2) = 8 を差し引き、65%の高さ
        let visible_lines = (term_height * 65 / 100).saturating_sub(8);
        let panel_inner_width = self.comment_panel_inner_width(term_width);

        // コメントパネルフォーカス中
        if self.comment_panel_open {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    let max_scroll = self.max_comment_panel_scroll(term_height, term_width);
                    self.comment_panel_scroll =
                        self.comment_panel_scroll.saturating_add(1).min(max_scroll);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.comment_panel_scroll = self.comment_panel_scroll.saturating_sub(1);
                }
                KeyCode::Char('n') => {
                    let prev_line = self.selected_line;
                    self.jump_to_next_comment();
                    if self.selected_line != prev_line {
                        self.comment_panel_scroll = 0;
                        self.selected_inline_comment = 0;
                        self.adjust_scroll(visible_lines);
                    }
                }
                KeyCode::Char('N') => {
                    let prev_line = self.selected_line;
                    self.jump_to_prev_comment();
                    if self.selected_line != prev_line {
                        self.comment_panel_scroll = 0;
                        self.selected_inline_comment = 0;
                        self.adjust_scroll(visible_lines);
                    }
                }
                KeyCode::Char(c) if c == self.config.keybindings.comment => {
                    self.enter_comment_input();
                }
                KeyCode::Char(c) if c == self.config.keybindings.suggestion => {
                    self.enter_suggestion_input();
                }
                KeyCode::Char('r') => {
                    if self.has_comment_at_current_line() {
                        self.enter_reply_input();
                    }
                }
                KeyCode::Tab => {
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
                }
                KeyCode::BackTab => {
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
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.diff_view_return_state = AppState::SplitViewDiff;
                    self.preview_return_state = AppState::DiffView;
                    self.state = AppState::DiffView;
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Esc | KeyCode::Char('q') => {
                    self.comment_panel_open = false;
                    self.comment_panel_scroll = 0;
                }
                _ => {}
            }
            return Ok(());
        }

        // pending_g_key: 2キーコマンド処理
        if self.pending_g_key {
            self.pending_g_key = false;
            match key.code {
                KeyCode::Char('d') => {
                    self.open_symbol_popup(terminal).await?;
                    return Ok(());
                }
                KeyCode::Char('f') => {
                    self.open_current_file_in_editor(terminal).await?;
                    return Ok(());
                }
                KeyCode::Char('g') => {
                    self.selected_line = 0;
                    self.scroll_offset = 0;
                    return Ok(());
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.diff_line_count > 0 {
                    self.selected_line =
                        (self.selected_line + 1).min(self.diff_line_count.saturating_sub(1));
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_line = self.selected_line.saturating_sub(1);
                self.adjust_scroll(visible_lines);
            }
            KeyCode::Char('g') => {
                self.pending_g_key = true;
            }
            KeyCode::Char('G') => {
                if self.diff_line_count > 0 {
                    self.selected_line = self.diff_line_count.saturating_sub(1);
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.jump_back();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.diff_line_count > 0 {
                    self.selected_line =
                        (self.selected_line + 20).min(self.diff_line_count.saturating_sub(1));
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected_line = self.selected_line.saturating_sub(20);
                self.adjust_scroll(visible_lines);
            }
            KeyCode::Char('n') => self.jump_to_next_comment(),
            KeyCode::Char('N') => self.jump_to_prev_comment(),
            KeyCode::Enter => {
                self.comment_panel_open = true;
                self.comment_panel_scroll = 0;
                self.selected_inline_comment = 0;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.diff_view_return_state = AppState::SplitViewDiff;
                self.preview_return_state = AppState::DiffView;
                self.state = AppState::DiffView;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.state = AppState::SplitViewFileList;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state = AppState::FileList;
            }
            _ => {}
        }

        Ok(())
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
                        RallyState::WaitingForClarification | RallyState::WaitingForPermission
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
                            rally_state.logs.push(LogEntry::new(
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
                        // Update state to prevent duplicate sends
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_permission = None;
                            rally_state.state = RallyState::Aborted;
                            rally_state.logs.push(LogEntry::new(
                                LogEventType::Info,
                                "Permission denied, aborting...".to_string(),
                            ));
                        }
                    }
                    RallyState::WaitingForClarification => {
                        // Send abort (skip clarification)
                        self.send_rally_command(OrchestratorCommand::Abort);
                        // Update state to prevent duplicate sends
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_question = None;
                            rally_state.state = RallyState::Aborted;
                            rally_state.logs.push(LogEntry::new(
                                LogEventType::Info,
                                "Clarification skipped, aborting...".to_string(),
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
        let answer = crate::editor::open_clarification_editor(&self.config.editor, question)?;

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
                    rally_state.logs.push(LogEntry::new(
                        LogEventType::Info,
                        format!("Clarification provided: {}", text),
                    ));
                }
            }
            _ => {
                // User cancelled (empty answer)
                self.send_rally_command(OrchestratorCommand::Abort);
                if let Some(ref mut rally_state) = self.ai_rally_state {
                    rally_state.logs.push(LogEntry::new(
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

        let diff = self
            .files()
            .iter()
            .filter_map(|f| f.patch.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");

        let context = Context {
            repo: self.repo.clone(),
            pr_number: self.pr_number,
            pr_title: pr.title.clone(),
            pr_body: pr.body.clone(),
            diff,
            working_dir: self.working_dir.clone(),
            head_sha: pr.head.sha.clone(),
            base_branch: pr.base.ref_name.clone(),
            external_comments: Vec::new(),
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
            last_visible_log_height: 10,
        });

        self.state = AppState::AiRally;

        // Spawn the orchestrator and store the abort handle
        let config = self.config.ai.clone();
        let repo = self.repo.clone();
        let pr_number = self.pr_number;

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
        // キャッシュを全削除
        let _ = crate::cache::invalidate_all_cache(&self.repo, self.pr_number);
        // コメントデータをクリア
        self.review_comments = None;
        self.discussion_comments = None;
        self.comments_loading = false;
        self.discussion_comments_loading = false;
        // PRデータを再取得
        self.retry_load();
    }

    async fn handle_diff_view_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        // シンボルポップアップ表示中
        if self.symbol_popup.is_some() {
            self.handle_symbol_popup_input(key, terminal).await?;
            return Ok(());
        }

        let term_size = terminal.size()?;
        let term_h = term_size.height as usize;
        let term_w = term_size.width as usize;
        let visible_lines = term_h.saturating_sub(8);
        let panel_inner_width = self.comment_panel_inner_width(term_w);

        // コメントパネルフォーカス中
        if self.comment_panel_open {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    let max_scroll = self.max_comment_panel_scroll(term_h, term_w);
                    self.comment_panel_scroll =
                        self.comment_panel_scroll.saturating_add(1).min(max_scroll);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.comment_panel_scroll = self.comment_panel_scroll.saturating_sub(1);
                }
                KeyCode::Char('n') => {
                    let prev_line = self.selected_line;
                    self.jump_to_next_comment();
                    if self.selected_line != prev_line {
                        self.comment_panel_scroll = 0;
                        self.selected_inline_comment = 0;
                        self.adjust_scroll(visible_lines);
                    }
                }
                KeyCode::Char('N') => {
                    let prev_line = self.selected_line;
                    self.jump_to_prev_comment();
                    if self.selected_line != prev_line {
                        self.comment_panel_scroll = 0;
                        self.selected_inline_comment = 0;
                        self.adjust_scroll(visible_lines);
                    }
                }
                KeyCode::Char(c) if c == self.config.keybindings.comment => {
                    self.enter_comment_input();
                }
                KeyCode::Char(c) if c == self.config.keybindings.suggestion => {
                    self.enter_suggestion_input();
                }
                KeyCode::Char('r') => {
                    if self.has_comment_at_current_line() {
                        self.enter_reply_input();
                    }
                }
                KeyCode::Tab => {
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
                }
                KeyCode::BackTab => {
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
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.state = self.diff_view_return_state;
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.comment_panel_open = false;
                    self.comment_panel_scroll = 0;
                }
                _ => {}
            }
            return Ok(());
        }

        // pending_g_key: 2キーコマンド処理
        if self.pending_g_key {
            self.pending_g_key = false;
            match key.code {
                KeyCode::Char('d') => {
                    self.open_symbol_popup(terminal).await?;
                    return Ok(());
                }
                KeyCode::Char('f') => {
                    self.open_current_file_in_editor(terminal).await?;
                    return Ok(());
                }
                KeyCode::Char('g') => {
                    // gg: 先頭行へジャンプ
                    self.selected_line = 0;
                    self.scroll_offset = 0;
                    return Ok(());
                }
                _ => {} // 無効な組み合わせ → フォールスルーして通常処理
            }
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.state = self.diff_view_return_state,
            KeyCode::Left | KeyCode::Char('h') => self.state = self.diff_view_return_state,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.diff_line_count > 0 {
                    self.selected_line =
                        (self.selected_line + 1).min(self.diff_line_count.saturating_sub(1));
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_line = self.selected_line.saturating_sub(1);
                self.adjust_scroll(visible_lines);
            }
            KeyCode::Char('g') => {
                self.pending_g_key = true;
            }
            KeyCode::Char('G') => {
                if self.diff_line_count > 0 {
                    self.selected_line = self.diff_line_count.saturating_sub(1);
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.jump_back();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.diff_line_count > 0 {
                    self.selected_line =
                        (self.selected_line + 20).min(self.diff_line_count.saturating_sub(1));
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected_line = self.selected_line.saturating_sub(20);
                self.adjust_scroll(visible_lines);
            }
            KeyCode::Char('n') => self.jump_to_next_comment(),
            KeyCode::Char('N') => self.jump_to_prev_comment(),
            KeyCode::Enter => {
                self.comment_panel_open = true;
                self.comment_panel_scroll = 0;
                self.selected_inline_comment = 0;
            }
            KeyCode::Char(c) if c == self.config.keybindings.comment => {
                self.enter_comment_input();
            }
            KeyCode::Char(c) if c == self.config.keybindings.suggestion => {
                self.enter_suggestion_input();
            }
            _ => {}
        }

        Ok(())
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
        let pr_number = self.pr_number;
        let line_number = ctx.line_number;

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some(rx);
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = github::create_review_comment(
                &repo,
                pr_number,
                &commit_id,
                &filename,
                line_number,
                &body,
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
        let pr_number = self.pr_number;
        let line_number = ctx.line_number;

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some(rx);
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = github::create_review_comment(
                &repo,
                pr_number,
                &commit_id,
                &filename,
                line_number,
                &body,
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
        let pr_number = self.pr_number;

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some(rx);
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

        self.input_mode = Some(InputMode::Comment(LineInputContext {
            file_index: self.selected_file,
            line_number,
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
        ui::restore_terminal(terminal)?;

        let body = crate::editor::open_review_editor(&self.config.editor)?;

        *terminal = ui::setup_terminal()?;

        if let Some(body) = body {
            github::submit_review(&self.repo, self.pr_number, action, &body).await?;
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
        self.pending_g_key = false;
        self.symbol_popup = None;
        self.update_diff_line_count();
        if self.review_comments.is_none() {
            self.load_review_comments();
        }
        self.update_file_comment_positions();
        self.ensure_diff_cache();
    }

    /// サジェスチョン入力を開始（組み込みTextArea）
    fn enter_suggestion_input(&mut self) {
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

        let original_code = line_info.line_content.clone();

        self.input_mode = Some(InputMode::Suggestion {
            context: LineInputContext {
                file_index: self.selected_file,
                line_number,
            },
            original_code: original_code.clone(),
        });
        // サジェスチョンは元コードを初期値として設定
        self.input_text_area.set_content(&original_code);
        self.preview_return_state = self.state;
        self.state = AppState::TextInput;
    }

    fn open_comment_list(&mut self) {
        self.state = AppState::CommentList;
        self.discussion_comment_detail_mode = false;
        self.discussion_comment_detail_scroll = 0;

        // Load review comments
        self.load_review_comments();
        // Load discussion comments
        self.load_discussion_comments();
    }

    fn load_review_comments(&mut self) {
        let cache_result = crate::cache::read_comment_cache(
            &self.repo,
            self.pr_number,
            crate::cache::DEFAULT_TTL_SECS,
        );

        let need_fetch = match cache_result {
            Ok(crate::cache::CacheResult::Hit(entry)) => {
                self.review_comments = Some(entry.comments);
                self.selected_comment = 0;
                self.comment_list_scroll_offset = 0;
                self.comments_loading = false;
                false
            }
            Ok(crate::cache::CacheResult::Stale(entry)) => {
                self.review_comments = Some(entry.comments);
                self.selected_comment = 0;
                self.comment_list_scroll_offset = 0;
                self.comments_loading = true;
                true
            }
            _ => {
                self.comments_loading = true;
                true
            }
        };

        if need_fetch {
            let (tx, rx) = mpsc::channel(1);
            self.comment_receiver = Some(rx);

            let repo = self.repo.clone();
            let pr_number = self.pr_number;

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

                // Cache and send
                if let Err(e) = crate::cache::write_comment_cache(&repo, pr_number, &all_comments) {
                    eprintln!("Warning: Failed to write comment cache: {}", e);
                }
                let _ = tx.send(Ok(all_comments)).await;
            });
        }
    }

    fn load_discussion_comments(&mut self) {
        let cache_result = crate::cache::read_discussion_comment_cache(
            &self.repo,
            self.pr_number,
            crate::cache::DEFAULT_TTL_SECS,
        );

        let need_fetch = match cache_result {
            Ok(crate::cache::CacheResult::Hit(entry)) => {
                self.discussion_comments = Some(entry.comments);
                self.selected_discussion_comment = 0;
                self.discussion_comments_loading = false;
                false
            }
            Ok(crate::cache::CacheResult::Stale(entry)) => {
                self.discussion_comments = Some(entry.comments);
                self.selected_discussion_comment = 0;
                self.discussion_comments_loading = true;
                true
            }
            _ => {
                self.discussion_comments_loading = true;
                true
            }
        };

        if need_fetch {
            let (tx, rx) = mpsc::channel(1);
            self.discussion_comment_receiver = Some(rx);

            let repo = self.repo.clone();
            let pr_number = self.pr_number;

            tokio::spawn(async move {
                match github::comment::fetch_discussion_comments(&repo, pr_number).await {
                    Ok(comments) => {
                        if let Err(e) = crate::cache::write_discussion_comment_cache(
                            &repo, pr_number, &comments,
                        ) {
                            eprintln!("Warning: Failed to write discussion comment cache: {}", e);
                        }
                        let _ = tx.send(Ok(comments)).await;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string())).await;
                    }
                }
            });
        }
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
            let _ = crate::editor::open_file_at_line(&self.config.editor, &path_str, line_number);
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
            &self.config.editor,
            &full_path,
            line_number.unwrap_or(1) as usize,
        );
        *terminal = crate::ui::setup_terminal()?;

        Ok(())
    }

    /// Diffキャッシュを構築または再利用
    pub fn ensure_diff_cache(&mut self) {
        let file_index = self.selected_file;

        // file_index を先に比較（O(1)）
        if let Some(ref cache) = self.diff_cache {
            if cache.file_index == file_index {
                // patch hash と comment_lines を比較（clone 前に参照比較）
                let Some(file) = self.files().get(file_index) else {
                    self.diff_cache = None;
                    return;
                };
                let Some(ref patch) = file.patch else {
                    self.diff_cache = None;
                    return;
                };
                let current_hash = hash_string(patch);
                if cache.patch_hash == current_hash
                    && cache.comment_lines == self.file_comment_lines
                {
                    return; // キャッシュ有効
                }
            }
        }

        // キャッシュ再構築
        let Some(file) = self.files().get(file_index) else {
            self.diff_cache = None;
            return;
        };
        let Some(patch) = file.patch.clone() else {
            self.diff_cache = None;
            return;
        };
        let filename = file.filename.clone();

        let lines = crate::ui::diff_view::build_diff_cache(
            &patch,
            &filename,
            &self.config.diff.theme,
            &self.file_comment_lines,
        );

        self.diff_cache = Some(DiffCache {
            file_index,
            patch_hash: hash_string(&patch),
            comment_lines: self.file_comment_lines.clone(),
            lines,
        });
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
}
