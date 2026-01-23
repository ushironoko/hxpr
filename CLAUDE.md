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

- **app.rs**: アプリケーション状態管理（`AppState`/`DataState`）、キーイベント処理
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
  - `diff_view.rs`: diff 表示画面
  - `comment_list.rs`: レビューコメント一覧画面
  - `ai_rally.rs`: AI Rally 画面
  - `help.rs`: ヘルプ画面
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
└─────────┘ └─────┬─────┘
                  │
        ┌─────────┼─────────┐
        │         │         │
        ▼         ▼         ▼
┌───────────┐ ┌────────┐ ┌─────┐
│NeedsClarif│ │NeedsPerm│ │Error│
└───────────┘ └────────┘ └─────┘
```

### Module Structure (src/ai/)

- **adapter.rs**: `AgentAdapter` トレイト定義、`Context`, `ReviewerOutput`, `RevieweeOutput` 型
- **adapters/claude.rs**: Claude Code CLI アダプター（`--output-format stream-json` でストリーミング）
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
  │     │     └── ClaudeAdapter → claude CLI (stream-json)
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
reviewer = "claude"
reviewee = "claude"
max_iterations = 10
timeout_secs = 600
reviewer_prompt = ""  # カスタムプロンプト（オプション）
reviewee_prompt = ""  # カスタムプロンプト（オプション）
```

### Usage

- TUI: `A` キーで AI Rally 開始
- CLI: `or --repo owner/repo --pr 123 --ai-rally`

### Known Limitations

1. **Permission/Clarificationフロー未完成**: `y`キーハンドリングはプレースホルダー
2. **--resume-rally未実装**: セッション永続化インフラは存在するが、再開機能は未実装

## Requirements

- GitHub CLI (`gh`) がインストール・認証済みであること
- Claude Code CLI (`claude`) がインストール・認証済みであること（AI Rally使用時）
- Rust 1.70+

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
