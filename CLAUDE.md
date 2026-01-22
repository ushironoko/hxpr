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
- **ui/**: TUI レンダリング（ratatui ベース）
  - `file_list.rs`: ファイル一覧画面
  - `diff_view.rs`: diff 表示画面
  - `comment_list.rs`: レビューコメント一覧画面
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

## Requirements

- GitHub CLI (`gh`) がインストール・認証済みであること
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
