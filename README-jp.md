# octorus

<p align="center">
  <img src="assets/banner.png" alt="octorus banner" width="600">
</p>

[![Crates.io](https://img.shields.io/crates/v/octorus.svg)](https://crates.io/crates/octorus)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

[English](./README.md)

Vim スタイルのキーバインドで操作する GitHub PR レビュー用 TUI ツール。

## 機能

- PR の変更ファイル一覧を閲覧
- シンタックスハイライト付きで diff を表示
- 特定の行にインラインコメントを追加
- コードサジェスチョンを追加
- レビューコメントの一覧表示と該当行へのジャンプ
- レビュー送信（Approve / Request Changes / Comment）
- インテリジェントキャッシュによる高速起動
- カスタマイズ可能なキーバインドとエディタ
- **AI Rally**: AI エージェントによる自動 PR レビュー＆修正サイクル

## 必要要件

- [GitHub CLI (gh)](https://cli.github.com/) - インストール・認証済みであること
- Rust 1.70+（ソースからビルドする場合）
- **AI Rally 機能を使用する場合**（オプション、いずれか）:
  - [Claude Code](https://claude.ai/code) - Anthropic の CLI ツール
  - [OpenAI Codex CLI](https://github.com/openai/codex) - OpenAI の CLI ツール

## インストール

```bash
cargo install octorus
```

または、ソースからビルド:

```bash
git clone https://github.com/ushironoko/octorus.git
cd octorus
cargo build --release
cp target/release/or ~/.local/bin/
```

## 使い方

```bash
or --repo owner/repo --pr 123
```

### オプション

| オプション | 説明 |
|--------|-------------|
| `-r, --repo <REPO>` | リポジトリ名（例: "owner/repo"） |
| `-p, --pr <PR>` | プルリクエスト番号 |
| `--refresh` | キャッシュを無視して強制更新 |
| `--cache-ttl <SECS>` | キャッシュ TTL（秒）（デフォルト: 300） |

### 設定ファイルの初期化

デフォルトの設定ファイルとプロンプトテンプレートを作成:

```bash
or init          # 設定ファイルを作成（既存の場合はスキップ）
or init --force  # 既存ファイルを上書き
```

作成されるファイル:
- `~/.config/octorus/config.toml` - メイン設定ファイル
- `~/.config/octorus/prompts/` - プロンプトテンプレートディレクトリ
  - `reviewer.md` - レビュワーエージェント用プロンプト
  - `reviewee.md` - レビュイーエージェント用プロンプト
  - `rereview.md` - 再レビュー用プロンプト

### キーバインド

#### ファイル一覧画面

| キー | 操作 |
|-----|--------|
| `j` / `↓` | 下に移動 |
| `k` / `↑` | 上に移動 |
| `Enter` | diff 画面を開く |
| `a` | PR を Approve |
| `r` | Request changes |
| `c` | Comment only |
| `C` | レビューコメント一覧を表示 |
| `R` | 強制リフレッシュ（キャッシュ破棄） |
| `A` | AI Rally を開始 |
| `?` | ヘルプを表示/非表示 |
| `q` | 終了 |

#### Diff 画面

| キー | 操作 |
|-----|--------|
| `j` / `↓` | 下に移動 |
| `k` / `↑` | 上に移動 |
| `n` | 次のコメントにジャンプ |
| `N` | 前のコメントにジャンプ |
| `Ctrl-d` | ページダウン |
| `Ctrl-u` | ページアップ |
| `c` | 現在行にコメントを追加 |
| `s` | 現在行にサジェスチョンを追加 |
| `q` / `Esc` | ファイル一覧に戻る |

**Note**: 既存のコメントがある行は `●` マーカーで表示されます。コメントのある行を選択すると、diff の下にコメント内容が表示されます。

#### コメント一覧画面

| キー | 操作 |
|-----|--------|
| `j` / `↓` | 下に移動 |
| `k` / `↑` | 上に移動 |
| `Enter` | ファイル/行にジャンプ |
| `q` / `Esc` | ファイル一覧に戻る |

## 設定

`or init` を実行してデフォルト設定ファイルを作成するか、手動で `~/.config/octorus/config.toml` を作成:

```toml
# コメント入力に使用するエディタ
editor = "vi"

[diff]
# diff 画面のシンタックスハイライトテーマ
theme = "base16-ocean.dark"

[keybindings]
approve = 'a'
request_changes = 'r'
comment = 'c'
suggestion = 's'

[ai]
# レビュワー/レビュイーに使用する AI エージェント
# サポート: "claude" (Claude Code), "codex" (OpenAI Codex CLI)
reviewer = "claude"
reviewee = "claude"

# 最大イテレーション回数
max_iterations = 10

# エージェント実行のタイムアウト（秒）
timeout_secs = 600

# カスタムプロンプトディレクトリ（デフォルト: ~/.config/octorus/prompts/）
# prompt_dir = "/custom/path/to/prompts"
```

### プロンプトテンプレートのカスタマイズ

AI Rally はカスタマイズ可能なプロンプトテンプレートを使用します。`or init` を実行してデフォルトテンプレートを生成し、必要に応じて編集してください:

```
~/.config/octorus/prompts/
├── reviewer.md    # レビュワーエージェント用プロンプト
├── reviewee.md    # レビュイーエージェント用プロンプト
└── rereview.md    # 再レビュー用プロンプト
```

テンプレートは `{{variable}}` 構文で変数置換をサポートしています:

| 変数 | 説明 | 使用可能なテンプレート |
|----------|-------------|--------------|
| `{{repo}}` | リポジトリ名（例: "owner/repo"） | すべて |
| `{{pr_number}}` | プルリクエスト番号 | すべて |
| `{{pr_title}}` | プルリクエストタイトル | すべて |
| `{{pr_body}}` | プルリクエスト本文 | reviewer |
| `{{diff}}` | PR の diff 内容 | reviewer |
| `{{iteration}}` | 現在のイテレーション番号 | すべて |
| `{{review_summary}}` | レビュワーからのサマリー | reviewee |
| `{{review_action}}` | レビューアクション（Approve/RequestChanges/Comment） | reviewee |
| `{{review_comments}}` | レビューコメント一覧 | reviewee |
| `{{blocking_issues}}` | ブロッキングイシュー一覧 | reviewee |
| `{{external_comments}}` | 外部ツールからのコメント | reviewee |
| `{{changes_summary}}` | 変更内容のサマリー | rereview |
| `{{updated_diff}}` | 修正後の diff | rereview |

## AI Rally

AI Rally は2つの AI エージェントによる自動 PR レビュー＆修正サイクルです:

- **Reviewer**: PR の diff を分析してレビューフィードバックを提供
- **Reviewee**: レビューフィードバックに基づいて問題を修正し、変更をコミット

### 動作フロー

```
┌─────────────────┐
│  Rally 開始     │  ファイル一覧画面で 'A' を押下
└────────┬────────┘
         ▼
┌─────────────────┐
│    Reviewer     │  AI が diff をレビュー
│ (Claude/Codex)  │  → PR にコメントを投稿
└────────┬────────┘
         │
    ┌────┴────┐
    │ Approve?│
    └────┬────┘
     No  │  Yes
         │   └──→ 完了 ✓
         ▼
┌─────────────────┐
│    Reviewee     │  AI が問題を修正
│ (Claude/Codex)  │  → ローカルに変更をコミット
└────────┬────────┘
         │
         ▼
    次のイテレーション
```

### 特徴

- **PR 統合**: レビューコメントは自動的に PR に投稿
- **外部 Bot サポート**: Copilot、CodeRabbit 等の Bot からのフィードバックを収集
- **安全な操作**: 危険な git 操作（`--force`、`reset --hard`）は禁止
- **セッション永続化**: Rally の状態は保存され、再開可能
- **インタラクティブフロー**: AI エージェントが確認や許可を求める際、対話的に応答可能
- **バックグラウンド実行**: `b` を押すと Rally をバックグラウンドで実行しながらファイル閲覧を継続可能

### キーバインド（AI Rally 画面）

| キー | 操作 |
|-----|--------|
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

## ライセンス

MIT
