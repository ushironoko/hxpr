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
use std::time::Instant;

/// コメントのdiff内位置を表す構造体
#[derive(Debug, Clone)]
pub struct CommentPosition {
    pub diff_line_index: usize,
    pub comment_index: usize,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    FileList,
    DiffView,
    CommentPreview,
    SuggestionPreview,
    CommentList,
    Help,
    AiRally,
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
pub struct SuggestionData {
    pub original_code: String,
    pub suggested_code: String,
    pub line_number: u32,
}

#[derive(Debug, Clone)]
pub struct CommentData {
    pub body: String,
    pub line_number: u32,
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
    pub selected_file: usize,
    pub selected_line: usize,
    pub diff_line_count: usize,
    pub scroll_offset: usize,
    pub pending_comment: Option<CommentData>,
    pub pending_suggestion: Option<SuggestionData>,
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
    // Cached diff lines (syntax highlighted)
    pub diff_cache: Option<DiffCache>,
    // Discussion comments (PR conversation)
    pub discussion_comments: Option<Vec<DiscussionComment>>,
    pub selected_discussion_comment: usize,
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
            selected_file: 0,
            selected_line: 0,
            diff_line_count: 0,
            scroll_offset: 0,
            pending_comment: None,
            pending_suggestion: None,
            config,
            should_quit: false,
            review_comments: None,
            selected_comment: 0,
            comment_list_scroll_offset: 0,
            comments_loading: false,
            file_comment_positions: vec![],
            file_comment_lines: HashSet::new(),
            diff_cache: None,
            discussion_comments: None,
            selected_discussion_comment: 0,
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
            selected_file: 0,
            selected_line: 0,
            diff_line_count,
            scroll_offset: 0,
            pending_comment: None,
            pending_suggestion: None,
            config,
            should_quit: false,
            review_comments: None,
            selected_comment: 0,
            comment_list_scroll_offset: 0,
            comments_loading: false,
            file_comment_positions: vec![],
            file_comment_lines: HashSet::new(),
            diff_cache: None,
            discussion_comments: None,
            selected_discussion_comment: 0,
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
                // Update comment positions if in diff view
                if self.state == AppState::DiffView {
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
                // Invalidate comment cache to force refresh on next open
                self.review_comments = None;
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
                    AppState::CommentPreview => self.handle_comment_preview_input(key)?,
                    AppState::SuggestionPreview => self.handle_suggestion_preview_input(key)?,
                    AppState::CommentList => self.handle_comment_list_input(key, terminal).await?,
                    AppState::Help => self.handle_help_input(key)?,
                    AppState::AiRally => self.handle_ai_rally_input(key, terminal).await?,
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
            KeyCode::Enter => {
                if !self.files().is_empty() {
                    self.state = AppState::DiffView;
                    self.selected_line = 0;
                    self.scroll_offset = 0;
                    self.update_diff_line_count();
                    self.update_file_comment_positions();
                    self.ensure_diff_cache();
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
            KeyCode::Char('C') => self.open_comment_list(),
            KeyCode::Char('R') => self.refresh_all(),
            KeyCode::Char('A') => self.resume_or_start_ai_rally(),
            KeyCode::Char('?') => self.state = AppState::Help,
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
                        // Clear pending permission
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_permission = None;
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
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_permission = None;
                            rally_state.logs.push(LogEntry::new(
                                LogEventType::Info,
                                "Permission denied, aborting...".to_string(),
                            ));
                        }
                    }
                    RallyState::WaitingForClarification => {
                        // Send abort (skip clarification)
                        self.send_rally_command(OrchestratorCommand::Abort);
                        if let Some(ref mut rally_state) = self.ai_rally_state {
                            rally_state.pending_question = None;
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

            // Estimate visible height (rough estimate, actual is calculated in UI)
            let visible_height = 10_usize; // approximate

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
        let visible_lines = terminal.size()?.height.saturating_sub(8) as usize;

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.state = AppState::FileList,
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
            KeyCode::Char(c) if c == self.config.keybindings.comment => {
                self.open_comment_editor(terminal).await?
            }
            KeyCode::Char(c) if c == self.config.keybindings.suggestion => {
                self.open_suggestion_editor(terminal).await?
            }
            KeyCode::Char('n') => self.jump_to_next_comment(),
            KeyCode::Char('N') => self.jump_to_prev_comment(),
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

    fn handle_comment_preview_input(&mut self, key: event::KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter => {
                if let Some(comment) = self.pending_comment.take() {
                    if let Some(file) = self.files().get(self.selected_file) {
                        if let Some(pr) = self.pr() {
                            let commit_id = pr.head.sha.clone();
                            let filename = file.filename.clone();
                            let repo = self.repo.clone();
                            let pr_number = self.pr_number;
                            let line_number = comment.line_number;
                            let body = comment.body;

                            // Start background submission
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
                    }
                }
                self.state = AppState::DiffView;
            }
            KeyCode::Esc => {
                self.pending_comment = None;
                self.state = AppState::DiffView;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_help_input(&mut self, key: event::KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                self.state = AppState::FileList;
            }
            _ => {}
        }
        Ok(())
    }

    async fn open_comment_editor(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let Some(file) = self.files().get(self.selected_file) else {
            return Ok(());
        };
        let Some(patch) = file.patch.as_ref() else {
            return Ok(());
        };

        // Get actual line number from diff
        let Some(line_info) = crate::diff::get_line_info(patch, self.selected_line) else {
            return Ok(());
        };

        // Only allow comments on Added or Context lines (not Removed/Header/Meta)
        if !matches!(
            line_info.line_type,
            crate::diff::LineType::Added | crate::diff::LineType::Context
        ) {
            return Ok(());
        }

        let Some(line_number) = line_info.new_line_number else {
            return Ok(());
        };

        let filename = file.filename.clone();

        ui::restore_terminal(terminal)?;

        let comment = crate::editor::open_comment_editor(
            &self.config.editor,
            &filename,
            line_number as usize,
        )?;

        *terminal = ui::setup_terminal()?;

        if let Some(body) = comment {
            self.pending_comment = Some(CommentData { body, line_number });
            self.state = AppState::CommentPreview;
        }
        Ok(())
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

    async fn open_suggestion_editor(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let Some(file) = self.files().get(self.selected_file) else {
            return Ok(());
        };
        let Some(patch) = file.patch.as_ref() else {
            return Ok(());
        };

        // Check if this line can have a suggestion
        let Some(line_info) = crate::diff::get_line_info(patch, self.selected_line) else {
            return Ok(());
        };

        // Only allow suggestions on Added or Context lines
        if !matches!(
            line_info.line_type,
            crate::diff::LineType::Added | crate::diff::LineType::Context
        ) {
            return Ok(());
        }

        let Some(new_line_number) = line_info.new_line_number else {
            return Ok(());
        };

        let filename = file.filename.clone();
        let original_code = line_info.line_content.clone();

        ui::restore_terminal(terminal)?;

        let suggested = crate::editor::open_suggestion_editor(
            &self.config.editor,
            &filename,
            new_line_number as usize,
            &original_code,
        )?;

        *terminal = ui::setup_terminal()?;

        if let Some(suggested_code) = suggested {
            self.pending_suggestion = Some(SuggestionData {
                original_code,
                suggested_code,
                line_number: new_line_number,
            });
            self.state = AppState::SuggestionPreview;
        }
        Ok(())
    }

    fn handle_suggestion_preview_input(&mut self, key: event::KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter => {
                if let Some(suggestion) = self.pending_suggestion.take() {
                    if let Some(file) = self.files().get(self.selected_file) {
                        if let Some(pr) = self.pr() {
                            let commit_id = pr.head.sha.clone();
                            let filename = file.filename.clone();
                            let body = format!(
                                "```suggestion\n{}\n```",
                                suggestion.suggested_code.trim_end()
                            );
                            let repo = self.repo.clone();
                            let pr_number = self.pr_number;
                            let line_number = suggestion.line_number;

                            // Start background submission
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
                    }
                }
                self.state = AppState::DiffView;
            }
            KeyCode::Esc => {
                self.pending_suggestion = None;
                self.state = AppState::DiffView;
            }
            _ => {}
        }
        Ok(())
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
            KeyCode::Char('q') => {
                self.state = AppState::FileList;
            }
            KeyCode::Esc => {
                self.state = AppState::FileList;
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
                            self.adjust_comment_scroll(visible_lines);
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
                    self.adjust_comment_scroll(visible_lines);
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

    fn adjust_comment_scroll(&mut self, visible_lines: usize) {
        if visible_lines == 0 {
            return;
        }
        if self.selected_comment < self.comment_list_scroll_offset {
            self.comment_list_scroll_offset = self.selected_comment;
        }
        if self.selected_comment >= self.comment_list_scroll_offset + visible_lines {
            self.comment_list_scroll_offset =
                self.selected_comment.saturating_sub(visible_lines) + 1;
        }
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
}
