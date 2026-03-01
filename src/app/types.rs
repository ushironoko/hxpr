use lasso::{Rodeo, Spur};
use ratatui::style::Style;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::ai::orchestrator::RallyEvent;
use crate::ai::RallyState;
use crate::diff::LineType;
use crate::github::comment::{DiscussionComment, ReviewComment};
use crate::github::{ChangedFile, PullRequest};

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
    /// 行の種類（背景色の決定に使用）
    pub line_type: LineType,
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

/// 複数行選択の状態
#[derive(Debug, Clone)]
pub struct MultilineSelection {
    /// 選択開始行（diff内のインデックス）。Shift+Enter押下時の行。
    pub anchor_line: usize,
    /// 選択終了行（diff内のインデックス）。カーソル移動で更新。
    pub cursor_line: usize,
}

impl MultilineSelection {
    /// 選択範囲の先頭行（小さい方）
    pub fn start(&self) -> usize {
        self.anchor_line.min(self.cursor_line)
    }

    /// 選択範囲の末尾行（大きい方）
    pub fn end(&self) -> usize {
        self.anchor_line.max(self.cursor_line)
    }
}

/// 行ベース入力のコンテキスト（コメント/サジェスチョン共通）
#[derive(Debug, Clone)]
pub struct LineInputContext {
    pub file_index: usize,
    pub line_number: u32,
    /// patch 内の position（1始まり）。GitHub API の `position` パラメータに対応。
    pub diff_position: u32,
    /// 複数行選択時の開始行番号（new file の行番号）
    pub start_line_number: Option<u32>,
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
pub(super) enum DiffViewVariant {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingApproveChoice {
    Ignore,
    Submit,
    Cancel,
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

/// PRファイルの viewed 変更結果
#[derive(Debug, Clone)]
pub(super) enum MarkViewedResult {
    Completed {
        marked_paths: Vec<String>,
        total_targets: usize,
        error: Option<String>,
        set_viewed: bool,
    },
}

/// ファイルウォッチャーのハンドル
///
/// `active` フラグで callback の処理を制御する。
/// スレッド自体は `_thread` で保持され、プロセス終了まで生存する。
pub struct WatcherHandle {
    pub(crate) active: Arc<AtomicBool>,
    pub(crate) _thread: std::thread::JoinHandle<()>,
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
    /// patch 内容を含む完全シグネチャ（バッチ diff 完了後に更新）
    pub local_file_patch_signatures: HashMap<String, u64>,
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

