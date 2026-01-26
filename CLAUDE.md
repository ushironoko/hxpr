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
cargo run -- --repo owner/repo --pr 123 --refresh  # キャッシュ無視

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
  ├── キャッシュ読み込み (同期)
  │     └── cache.rs: read_cache() → ~/.cache/octorus/{repo}_{pr}.json
  ├── App 初期化
  │     └── app.rs: new_loading() または new_with_cache()
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
  - `comment_list.rs`: レビューコメント一覧画面
  - `ai_rally.rs`: AI Rally 画面
  - `help.rs`: ヘルプ画面
- **diff.rs**: diff パース、行分類（`LineType`）、ハンク解析
- **cache.rs**: キャッシュ管理（JSON ファイル、TTL ベース）
  - PRデータ: `~/.cache/octorus/{repo}_{pr}.json`
  - コメント: `~/.cache/octorus/{repo}_{pr}_comments.json`
- **loader.rs**: バックグラウンドデータ取得（`tokio::spawn`）
- **config.rs**: 設定ファイル読み込み（`~/.config/octorus/config.toml`）
- **editor/**: 外部エディタ連携（コメント入力）

### Key State Machines

**AppState** (UI 状態):
- `FileList` → `DiffView` → `CommentPreview` / `SuggestionPreview`
- `FileList` → `CommentList` → `DiffView` (コメントジャンプ)
- `FileList` → `AiRally` (AI Rally 画面)
- `Help` (トグル)

**Diff View Features**:
- インラインコメント表示: コメントがある行は `●` マーカーで表示
- コメントパネル: コメントのある行を選択すると下部にコメント内容を表示
- `n` / `N` でコメント間をジャンプ（ラップなし、先頭にスクロール）

**DataState** (データ状態):
- `Loading` → `Loaded { pr, files }` or `Error(String)`

### Async Pattern

キャッシュヒット時は即座に UI 表示し、バックグラウンドで更新チェックを行う lazy loading パターン:

**PRデータ取得**:
1. `main.rs`: 同期的にキャッシュ読み込み
2. `App::new_with_cache()` で即座にデータ表示
3. `loader::fetch_pr_data()` がバックグラウンドで更新チェック
4. `App::poll_data_updates()` が mpsc チャンネル経由で更新を受信

**コメント取得**:
1. `App::open_comment_list()`: キャッシュ確認 → 即座に画面遷移
2. キャッシュヒット: 即座に表示、API呼び出しなし
3. キャッシュ古い/なし: バックグラウンドで取得
4. `App::poll_comment_updates()` が mpsc チャンネル経由で更新を受信

### Diff Cache

シンタックスハイライト済みの diff 行をキャッシュし、スクロール時の再計算を回避:

- `DiffCache`: ファイルインデックス、patch ハッシュ、コメント行セット、キャッシュ済み行データを保持
- `CachedDiffLine`: `Vec<Span<'static>>` を保持（REVERSED 修飾子なし）
- `App::ensure_diff_cache()`: キャッシュの有効性を確認し、必要に応じて再構築
- キャッシュ無効化条件: ファイル変更、patch 内容変更、コメント行変更

**更新タイミング**:
- `handle_file_list_input(Enter)`: ファイル選択時
- `poll_comment_updates()`: コメント取得完了時
- `jump_to_comment()`: コメントジャンプ時

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
        └── run_reviewer(fetch updated diff via `gh pr diff`, include fix summary)
              └── Re-review with current PR state
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
