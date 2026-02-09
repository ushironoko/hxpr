# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

octorus は GitHub PR レビュー用の TUI ツール。Vim スタイルのキーバインドで操作し、`gh` CLI を介して GitHub API と通信する。

## Build & Development Commands

```bash
# ビルド
cargo build
cargo build --release

# 実行
cargo run -- --repo owner/repo --pr 123

# テスト
cargo test
cargo test <test_name>  # 単一テスト実行

# リリースビルド後のバイナリ
./target/release/or --repo owner/repo --pr 123
```

## Architecture

### Data Flow

```
main.rs
  ├── App 初期化
  │     └── app.rs: new_loading() [常に Loading 状態で開始]
  └── バックグラウンドタスク起動
        └── loader.rs: fetch_pr_data()
              └── github/client.rs: gh_command() → gh CLI 実行
```

### Module Structure

- **app.rs**: アプリケーション状態管理（`AppState`/`DataState`/`DiffCache`）、キーイベント処理
- **github/**: GitHub API 通信
  - `client.rs`: `gh` CLI のラッパー（`gh_command`, `gh_api`, `gh_api_post`）
  - `pr.rs`: PR 情報取得、レビュー送信
  - `comment.rs`: レビューコメント取得・作成
- **ai/**: AI Rally 機能（詳細は後述の「AI Rally Feature」セクション参照）
  - `adapter.rs`: AgentAdapter トレイト
  - `adapters/claude.rs`: Claude Code アダプター
  - `orchestrator.rs`: ラリーオーケストレーター
  - `prompts.rs`: プロンプトテンプレート
  - `session.rs`: セッション永続化
- **ui/**: TUI レンダリング（ratatui ベース）
  - `file_list.rs`: ファイル一覧画面
  - `diff_view.rs`: diff 表示画面（インラインコメント表示、キャッシュ管理）
  - `split_view.rs`: 分割プレビュー画面（ファイル一覧 + diff プレビュー）
  - `comment_list.rs`: レビューコメント一覧画面
  - `ai_rally.rs`: AI Rally 画面
  - `help.rs`: ヘルプ画面
- **diff.rs**: diff パース、行分類（`LineType`）、ハンク解析
- **cache.rs**: インメモリセッションキャッシュ（`SessionCache`、LRU eviction 付き）
  - `PrData`: PRデータ（`Box<PullRequest>` + `Vec<ChangedFile>`）
  - レビューコメント / ディスカッションコメント: PRデータのライフサイクルに連動
  - `sanitize_repo_name()`: パストラバーサル防止（AI Rally 等で使用）
  - `cache_dir()` / `cleanup_rally_sessions()`: AI Rally セッション管理
- **loader.rs**: バックグラウンドデータ取得（`tokio::spawn`）
- **config.rs**: 設定ファイル読み込み（`~/.config/octorus/config.toml`）
- **editor/**: 外部エディタ連携（コメント入力）

### Key State Machines

**AppState** (UI 状態):

```
FileList ──[Enter/→/l]──> SplitViewDiff ──[Tab/→/l]──> DiffView
    ▲                          │  │                        │
    │                          │  │[←/h]                   │[q/Esc/←/h]
    │[q/Esc]                   │  ▼                        │
    └──────────────────────────┘ SplitViewFileList         │
                                   │  ▲                    │
                                   │  │[Enter/→/l]         │
                                   │  └────────────────────┘
                                   │[q/Esc]
                                   ▼
                                FileList
```

- `DiffView` → `CommentPreview` / `SuggestionPreview`（戻り先は `preview_return_state` で管理）
- `FileList` / `SplitViewFileList` → `CommentList`（戻り先は `previous_state` で管理）
- `CommentList` → `DiffView` (コメントジャンプ、`diff_view_return_state = FileList`)
- `FileList` → `AiRally` (AI Rally 画面)
- `Help` (戻り先は `previous_state` で管理)

**Diff View Features**:
- インラインコメント表示: コメントがある行は `●` マーカーで表示
- コメントパネル: コメントのある行を選択すると下部にコメント内容を表示
- `n` / `N` でコメント間をジャンプ（ラップなし、先頭にスクロール）

**DataState** (データ状態):
- `Loading` → `Loaded { pr, files }` or `Error(String)`

### Performance Optimizations

最適化は5つのレイヤーで構成される。上位レイヤーほどユーザーに近く、下位ほど基盤的。

```
┌─────────────────────────────────────────────────────────────┐
│ L1: インメモリセッションキャッシュ（LRU eviction 付き）        │
│   cache.rs: SessionCache — PRデータ/コメントを HashMap で管理  │
└─────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│ L2: プリフェッチ（バックグラウンド事前構築）                    │
│   app.rs: start_prefetch_all_files() → 全ファイル事前構築     │
└─────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│ L3: DiffCache（インメモリ 3段階ルックアップ）                  │
│   app.rs: ensure_diff_cache() → 現在/ストア/新規構築          │
└─────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│ L4: ParserPool（パーサー/クエリ再利用）                       │
│   syntax/parser_pool.rs: 言語ごとに Parser/Query を1つ保持    │
└─────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│ L5: ハイライト構築最適化                                      │
│   syntax/highlighter.rs: 全体パース・文字列インターニング等   │
└─────────────────────────────────────────────────────────────┘
```

#### 定数一覧

| 定数 | 値 | 場所 | 用途 |
|------|----|------|------|
| `MAX_PR_CACHE_ENTRIES` | 5 | `cache.rs` | セッションキャッシュPRデータ上限（LRU） |
| `MAX_HIGHLIGHTED_CACHE_ENTRIES` | 50 | `app.rs` | ハイライトキャッシュストア上限 |
| `MAX_PREFETCH_FILES` | 50 | `app.rs` | プリフェッチ対象ファイル上限 |

---

#### L1: インメモリセッションキャッシュ（cache.rs: SessionCache）

PRデータとコメントをインメモリの `HashMap` で管理。ファイルI/Oは一切行わない。
プロセス終了時にメモリ解放されるため明示的なクリーンアップは不要。

**データ構造**:

```rust
SessionCache {
    pr_data: HashMap<PrCacheKey, PrData>,        // PRデータ本体
    access_order: Vec<PrCacheKey>,               // LRU 追跡（末尾が最新）
    review_comments: HashMap<PrCacheKey, Vec<ReviewComment>>,
    discussion_comments: HashMap<PrCacheKey, Vec<DiscussionComment>>,
}

PrCacheKey { repo: String, pr_number: u32 }

PrData {
    pr: Box<PullRequest>,       // Arc不使用: シングルスレッド設計
    files: Vec<ChangedFile>,    // Arc不使用: clone()で分配
    pr_updated_at: String,
}
```

**設計方針**:
- `Arc` ではなく `Box`/`Vec` + `clone()` を使用。メインスレッドのイベントループからのみアクセスされるため、スレッド間共有は不要
- `DataState::Loaded` と `SessionCache` の両方にデータが必要な場合は `clone()` で分配（PR更新時のみ発生）
- LRU eviction: 最大 `MAX_PR_CACHE_ENTRIES`（5）件。超過時は最も古いエントリを削除
- コメントは対応する `pr_data` が存在するキーにのみ保存可能（ライフサイクル連動）

**戦略**:

```
[起動（PR番号指定）]
  → App::new_loading() [常に Loading 状態で開始]
  + spawn { FetchMode::Fresh }  [APIから取得]
  → メインループ: poll_data_updates()
    → mpsc 経由で受信 → session_cache.put_pr_data() + DataState::Loaded に遷移

[PR一覧からPR選択（select_pr）]
  → session_cache.get_pr_data()
    → Some → DataState::Loaded に即座遷移 [インメモリキャッシュヒット]
           + spawn { FetchMode::CheckUpdate(pr_updated_at) } [バックグラウンドで鮮度チェック]
    → None → DataState::Loading
           + spawn { FetchMode::Fresh } [APIから取得]

[Rキー（refresh_all）]
  → session_cache.invalidate_all() [全キャッシュ破棄]
  → retry_load() → FetchMode::Fresh [APIから再取得]
```

**コメント取得**:
1. `App::open_comment_list()`: `session_cache` 確認 → 即座に画面遷移
2. キャッシュヒット: 即座に表示、API呼び出しなし
3. キャッシュなし: バックグラウンドで取得 → `poll_comment_updates()` 経由で受信

**クロスPRキャッシュ汚染防止**: `PrReceiver<T> = Option<(u32, mpsc::Receiver<T>)>` で受信データの発信元PR番号を追跡。現在のPRと異なるデータはキャッシュのみに格納し、UI状態には反映しない。

**セキュリティ**: `sanitize_repo_name()` でパストラバーサル攻撃を防止（AI Rally のセッションパスで使用）

---

#### L2: プリフェッチ（app.rs: start_prefetch_all_files）

PRデータロード完了時に、全ファイルのシンタックスハイライトキャッシュをバックグラウンドで事前構築。

**開始トリガー**: PRデータロード完了時（`poll_data_updates()` でデータ到着時、または `select_pr()` でインメモリキャッシュヒット時）

```
[PRデータロード完了]
  → start_prefetch_all_files()
    → 未キャッシュファイル収集（patch ありのみ、上限 MAX_PREFETCH_FILES 件）
    → spawn_blocking {
        単一 ParserPool で全ファイルの build_diff_cache() を順次実行
        → mpsc::channel 経由で完成したキャッシュを送信
      }
  → メインループ: poll_prefetch_updates()
    → 受信した DiffCache を highlighted_cache_store に格納
    → 現在表示中のファイルはスキップ（重複防止）
    → 既にストアにあるファイルもスキップ
    → サイズ超過時: 現在選択中ファイルから最も遠いエントリを削除（距離ベース eviction）
```

---

#### L3: DiffCache（app.rs: ensure_diff_cache）

シンタックスハイライト済みの diff 行をインメモリキャッシュし、スクロール時の再計算を回避。

**データ構造**:

```rust
DiffCache {
    file_index: usize,              // キャッシュ対象のファイルインデックス
    patch_hash: u64,                // patch 内容のハッシュ（変更検出用）
    lines: Vec<CachedDiffLine>,     // パース済み行データ
    interner: Rodeo,                // 文字列インターナー（キャッシュ内で共有）
    highlighted: bool,              // ハイライト済みフラグ（false=プレーン）
}

CachedDiffLine {
    spans: Vec<InternedSpan>,       // REVERSED 修飾子なし（コメントマーカーはレンダリング時に挿入）
}

InternedSpan {
    content: Spur,  // インターン済み文字列参照（4 bytes）
    style: Style,   // スタイル情報（8 bytes）
}
```

**コメントマーカー（● ）**: キャッシュ構築時ではなくレンダリング時（`render_cached_lines`）にイテレータ合成で挿入。
これにより `DiffCache` が `comment_lines` に依存せず、プリフェッチキャッシュがコメント取得後も有効。

**3段階ルックアップ（ensure_diff_cache）**:

```
1. 現在の diff_cache が有効か確認（O(1)）
   → file_index, patch_hash を照合
   → 有効ならそのまま使用

2. highlighted_cache_store にハイライト済みキャッシュがあるか確認
   → patch_hash を照合
   → 有効なら復元（ファイル遷移時の即座復元）

3. キャッシュミス: 2段階構築
   → 即座: build_plain_diff_cache() [~1ms、diff色分けのみ]
   → BG:   spawn_blocking { build_diff_cache() } [シンタックスハイライト]
           → poll_diff_cache_updates() で受信してスワップ
```

**キャッシュ無効化条件**: `file_index` / `patch_hash` のいずれかが不一致

**更新トリガー**:
- `handle_file_list_input(Enter)`: ファイル選択時
- `poll_comment_updates()`: コメント取得完了時
- `jump_to_comment()`: コメントジャンプ時

**Stale防止**:
- `poll_diff_cache_updates()`: バリデーション（file_index, patch_hash, DataState）で stale キャッシュを破棄
- PR遷移時: `diff_cache_receiver`, `prefetch_receiver`, `highlighted_cache_store` をすべてクリア

---

#### L4: ParserPool（syntax/parser_pool.rs）

tree-sitter の Parser と Query を言語ごとにプールし、生成コストを回避。

```rust
ParserPool {
    parsers: HashMap<SupportedLanguage, Parser>,  // ~200KB/parser、遅延生成
    queries: HashMap<SupportedLanguage, Query>,   // コンパイル済みクエリキャッシュ
}
```

- `get_or_create(ext)`: 拡張子からパーサーを取得/生成
- `get_or_create_query(lang)`: ハイライトクエリを取得/コンパイル（injection 処理で特に効果大）

**効果**: Svelte（3クエリ: Svelte/TS/CSS）や Vue（3クエリ: Vue/TS/CSS）で大幅高速化。プリフェッチ時は単一 ParserPool を全ファイルで共有。

---

#### L5: ハイライト構築最適化（syntax/highlighter.rs, ui/diff_view.rs）

**5a. 全体パース + 行ごと適用（collect_line_highlights）**

従来の「行ごとにクエリ実行」ではなく、ソース全体を1回クエリ実行し、結果を行ごとにマッピング。

```
[build_diff_cache]
  → build_combined_source_for_highlight_with_priming()
    → added/context 行のみ抽出（removed 行を除外して構文エラー回避）
    → Vue/Svelte: priming tag 追加（<script lang="ts"> 等）
  → Highlighter::for_file() → ThemeStyleCache 構築
  → highlighter.parse_source() → Tree
  → collect_line_highlights_with_injections()
    → 親言語のハイライト収集（1回のクエリ実行）
    → 各 injection 範囲を別パーサーで処理（TS/CSS 等）
    → マージして LineHighlights 返却
  → build_lines_with_cst()
    → 各 diff 行にハイライトを適用 → InternedSpan 生成
```

**5b. 文字列インターニング（lasso::Rodeo）**

`let`, `const`, `fn` 等の重複トークンを `Spur`（4 bytes インデックス）で共有。DiffCache ごとに Rodeo を保持。

**5c. スタイルキャッシュ（ThemeStyleCache）**

`HashMap<&'static str, Style>` で capture 名 → Style を事前計算。各 capture で O(1) ルックアップ。

**5d. injection 処理（Svelte/Vue）**

`collect_line_highlights_with_injections()` で `<script lang="ts">`/`<style>` 内のコードを別パーサーでハイライト。親言語のハイライトとマージ。

## AI Rally Feature

AI Rally は2つのAIエージェント（Reviewer/Reviewee）がPRに対してレビューと修正を自動で繰り返す機能。

### State Transition

```
┌─────────────┐
│ Initializing│
└──────┬──────┘
       │ Context loaded
       ▼
┌──────────────────┐
│ ReviewerReviewing│◄─────────────────────────┐
└────────┬─────────┘                          │
         │                                    │
    ┌────┴────┐                               │
    │         │                               │
    ▼         ▼                               │
┌───────┐ ┌────────────┐                      │
│Approve│ │RequestChg/ │                      │
│       │ │Comment     │                      │
└───┬───┘ └─────┬──────┘                      │
    │           │                             │
    ▼           ▼                             │
┌─────────┐ ┌───────────┐    Fix completed    │
│Completed│ │RevieweeFix├─────────────────────┘
└─────────┘ └─────┬─────┘                     ▲
                  │                           │
        ┌─────────┼─────────┐                 │
        │         │         │                 │
        ▼         ▼         ▼                 │
┌───────────┐ ┌────────┐ ┌─────┐              │
│NeedsClarif│ │NeedsPerm│ │Error│              │
└─────┬─────┘ └───┬────┘ └─────┘              │
      │ y: answer │ y: approve                │
      └───────────┴───────────────────────────┘
```

### Module Structure (src/ai/)

- **adapter.rs**: `AgentAdapter` トレイト定義、`Context`, `ReviewerOutput`, `RevieweeOutput` 型
- **adapters/**: エージェントアダプター実装
  - `mod.rs`: `create_adapter()` ファクトリ関数
  - `claude.rs`: Claude Code CLI アダプター（`--output-format stream-json` でストリーミング）
  - `codex.rs`: OpenAI Codex CLI アダプター（`--json` でストリーミング）
- **orchestrator.rs**: ラリーオーケストレーター、状態管理、イベント送信
- **prompts.rs**: レビュワー/レビュイー用プロンプトテンプレート
- **session.rs**: セッション永続化（`~/.cache/octorus/rally/{repo}_{pr}/`）
- **schemas/**: 構造化出力用JSONスキーマ（`reviewer.json`, `reviewee.json`）

### Data Flow

```
Orchestrator.run()
  │
  ├── Iteration 1
  │     ├── run_reviewer(PR diff from GitHub)
  │     │     └── AgentAdapter (Claude or Codex) → CLI (stream-json)
  │     │           └── NDJSON events → RallyEvent::AgentThinking/ToolUse/Text
  │     │
  │     ├── ReviewerOutput saved to history/001_review.json
  │     │
  │     └── run_reviewee(ReviewerOutput embedded in prompt)
  │           └── Edit/Write local files (commits locally, no push)
  │           └── RevieweeOutput saved to history/001_fix.json
  │
  └── Iteration 2+
        └── fetch_current_diff()
              ├── 1. git diff origin/{base_branch}...HEAD (ローカル優先)
              │     └── 未プッシュのローカル変更を検出
              └── 2. gh pr diff (GitHub API フォールバック)
                    └── push 済み or ローカル diff が空の場合に使用
        └── run_reviewer(updated diff, include fix summary)
              └── Re-review with current state
```

### Tool Permissions

| Agent | 許可 | 禁止 |
|-------|------|------|
| Reviewer | Read, Glob, Grep, `gh pr view/diff/checks`, `gh api` (GET) | Write, Edit, git push |
| Reviewee | Read, Edit, Write, `git status/add/commit/diff/log/show/branch/switch/stash`, `cargo build/test/check/clippy/fmt`, `npm/pnpm/bun install/test/run` | **git push**, git checkout, git restore, cargo/npm publish |

**Note**: Reviewee commits changes locally but does NOT push. The user must push manually after reviewing the changes.

### Storage

```
~/.cache/octorus/rally/{repo}_{pr}/
├── session.json      # 現在の状態（iteration, state）
├── context.json      # PRコンテキスト
├── history/
│   ├── 001_review.json
│   ├── 001_fix.json
│   ├── 002_review.json
│   └── ...
└── logs/
    └── *.log
```

### Configuration

```toml
# ~/.config/octorus/config.toml
[ai]
# サポート: "claude" (Claude Code), "codex" (OpenAI Codex CLI)
reviewer = "claude"
reviewee = "claude"
max_iterations = 10
timeout_secs = 600
# prompt_dir = "/custom/path/to/prompts"  # カスタムプロンプトディレクトリ

# reviewer 用の追加ツール (Claude only)
# Claude Code の --allowedTools 形式で指定
# reviewer_additional_tools = []

# reviewee 用の追加ツール (Claude only)
# reviewee_additional_tools = ["Skill", "Bash(git push:*)"]

# 例:
#   - "Skill"                      : Claude Code スキル実行
#   - "WebFetch"                   : URL コンテンツ取得
#   - "WebSearch"                  : Web 検索
#   - "Bash(git push:*)"           : git push
#   - "Bash(gh api --method POST:*)": GitHub API POST
#
# NOTE: git push はデフォルトで無効。リモートへの自動プッシュを許可する場合のみ設定
```

**推奨構成**: Codex は細粒度のツール制御ができないため、以下の構成を推奨:

```toml
[ai]
reviewer = "codex"   # Codex は read-only sandbox で安全
reviewee = "claude"  # Claude は allowedTools で細かく制御可能
reviewee_additional_tools = ["Skill"]  # 必要に応じて追加
```

### Usage

- TUI: `A` キーで AI Rally 開始
- CLI: `or --repo owner/repo --pr 123 --ai-rally`

### Known Limitations

1. **--resume-rally未実装**: セッション永続化インフラは存在するが、プロセス再起動後の再開機能は未実装

### Clarification/Permission Flow

AI Rallyでレビュイーが`NeedsClarification`または`NeedsPermission`を返した時のフロー:

1. **WaitingForClarification**: ユーザーに質問を表示し、`y`でエディタ入力、`n`でスキップ（abort）
2. **WaitingForPermission**: ユーザーにアクション/理由を表示し、`y`で承認、`n`で拒否

### TUI Keybindings (AI Rally View)

| キー | 操作 |
|------|------|
| `j` / `↓` | ログ内を下に移動 |
| `k` / `↑` | ログ内を上に移動 |
| `Enter` | ログ詳細を表示 |
| `g` | 先頭にジャンプ |
| `G` | 末尾にジャンプ |
| `b` | バックグラウンド実行（ファイル一覧に戻る） |
| `y` | 許可を付与 / 回答を入力 |
| `n` | 許可を拒否 / スキップ |
| `r` | リトライ（エラー時） |
| `q` / `Esc` | Rally を中止して終了 |

## Requirements

- GitHub CLI (`gh`) がインストール・認証済みであること
- Rust 1.70+
- **AI Rally使用時**（いずれか）:
  - Claude Code CLI (`claude`) がインストール・認証済み
  - OpenAI Codex CLI (`codex`) がインストール・認証済み

## Dependency Version Policy

Cargo.toml での依存クレートは **必ず正確なバージョン（exactバージョン）を指定する**。

```toml
# Good - 正確なバージョン指定
anyhow = "1.0.100"
tokio = { version = "1.49.0", features = ["rt-multi-thread"] }

# Bad - 曖昧なバージョン指定
anyhow = "1"
tokio = { version = "1", features = ["rt-multi-thread"] }
```

新しいクレートを追加する際は `cargo add <crate>` 後に `cargo tree --depth 1` で正確なバージョンを確認し、Cargo.toml を修正すること。
