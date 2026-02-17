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
- ファイル一覧と diff プレビューの分割表示（Split View）
- シンタックスハイライト付きで diff を表示
- 特定の行にインラインコメントを追加
- コードサジェスチョンを追加
- レビューコメントの一覧表示と該当行へのジャンプ
- レビュー送信（Approve / Request Changes / Comment）
- インテリジェントキャッシュによる高速起動
- 作業ツリー変更をリアルタイムで監視して `git diff` を即時プレビュー
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
# 1. 設定ファイルを初期化（AI Rally を使う場合は推奨）
or init

# 2. 現在のリポジトリの PR 一覧を開く（git remote から自動検出）
or

# 3. 特定の PR を開く
or --repo owner/repo --pr 123

# 4. AI Rally を開始（PR 選択後に自動開始）
or --ai-rally

# 5. ローカルの変更をリアルタイムで確認
or --local
```

### オプション

| オプション | 説明 |
|--------|-------------|
| `-r, --repo <REPO>` | リポジトリ名（例: "owner/repo"） |
| `-p, --pr <PR>` | プルリクエスト番号 |
| `--ai-rally` | AI Rally モードを直接開始 |
| `--working-dir <DIR>` | AI エージェントの作業ディレクトリ（デフォルト: カレントディレクトリ） |
| `--local` | GitHub 取得をせず、`HEAD` との差分を表示 |
| `--auto-focus` | ローカルモード時に差分更新があったファイルへ自動フォーカス |

### サブコマンド

| サブコマンド | 説明 |
|------------|-------------|
| `or init` | 設定ファイルとプロンプトテンプレートを初期化 |
| `or init --force` | 既存の設定ファイルを上書き |
| `or clean` | AI Rally セッションデータを削除 |

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
| `Enter` / `→` / `l` | Split View を開く |
| `a` | PR を Approve |
| `r` | Request changes |
| `c` | Comment only |
| `C` | レビューコメント一覧を表示 |
| `R` | 強制リフレッシュ（キャッシュ破棄） |
| `A` | AI Rally を開始 |
| `?` | ヘルプを表示/非表示 |
| `q` | 終了 |

#### Split View

Split View はファイル一覧（左 35%）と diff プレビュー（右 65%）を横並びで表示します。フォーカスされているペインは黄色のボーダーでハイライトされます。

**ファイル一覧フォーカス時:**

| キー | 操作 |
|-----|--------|
| `j` / `↓` | ファイル選択を移動（diff が追従） |
| `k` / `↑` | ファイル選択を移動（diff が追従） |
| `Enter` / `→` / `l` | diff ペインにフォーカス |
| `←` / `h` / `q` | ファイル一覧に戻る |

**diff フォーカス時:**

| キー | 操作 |
|-----|--------|
| `j` / `↓` | diff をスクロール |
| `k` / `↑` | diff をスクロール |
| `gd` | 定義へジャンプ |
| `gf` | $EDITOR でファイルを開く |
| `gg` / `G` | 先頭/末尾にジャンプ |
| `Ctrl-o` | 前の位置に戻る |
| `Ctrl-d` | ページダウン |
| `Ctrl-u` | ページアップ |
| `n` | 次のコメントにジャンプ |
| `N` | 前のコメントにジャンプ |
| `Enter` | コメントパネルを開く |
| `Tab` / `→` / `l` | フルスクリーン diff 画面を開く |
| `←` / `h` | ファイル一覧にフォーカス |
| `q` | ファイル一覧に戻る |

#### Diff 画面

| キー | 操作 |
|-----|--------|
| `j` / `↓` | 下に移動 |
| `k` / `↑` | 上に移動 |
| `gd` | 定義へジャンプ |
| `gf` | $EDITOR でファイルを開く |
| `gg` / `G` | 先頭/末尾にジャンプ |
| `Ctrl-o` | 前の位置に戻る |
| `n` | 次のコメントにジャンプ |
| `N` | 前のコメントにジャンプ |
| `Ctrl-d` | ページダウン |
| `Ctrl-u` | ページアップ |
| `Enter` | コメントパネルを開く |
| `←` / `h` / `q` / `Esc` | 前の画面に戻る |

**Note**: 既存のコメントがある行は `●` マーカーで表示されます。コメントのある行を選択すると、diff の下にコメント内容が表示されます。

**コメントパネル（フォーカス時）:**

| キー | 操作 |
|-----|--------|
| `j` / `k` | パネルをスクロール |
| `c` | コメントを追加 |
| `s` | サジェスチョンを追加 |
| `r` | コメントに返信 |
| `Tab` / `Shift-Tab` | 返信対象を選択 |
| `n` / `N` | 次/前のコメントにジャンプ |
| `Esc` / `q` | パネルを閉じる |

#### 入力モード（コメント/サジェスチョン/リプライ）

コメント、サジェスチョン、リプライを追加する際は、組み込みテキスト入力モードに入ります:

| キー | 操作 |
|-----|--------|
| `Ctrl+S` | 送信 |
| `Esc` | キャンセル |

複数行の入力が可能です。`Enter` で改行を挿入できます。

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
# レビュー本文（Approve/Request Changes/Comment）入力に使用するエディタ
editor = "vi"

[diff]
# diff 画面のシンタックスハイライトテーマ
theme = "base16-ocean.dark"

[keybindings]
# 設定可能なすべてのキーについては「設定可能なキーバインド」セクションを参照
approve = "a"
request_changes = "r"
comment = "c"
suggestion = "s"

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

# reviewer 用の追加ツール（Claude only）
# Claude Code の --allowedTools 形式で指定
# reviewer_additional_tools = []

# reviewee 用の追加ツール（Claude only）
# 例: "Skill", "WebFetch", "WebSearch", "Bash(git push:*)"
# reviewee_additional_tools = ["Skill", "Bash(git push:*)"]
```

### 設定可能なキーバインド

すべてのキーバインドは `[keybindings]` セクションでカスタマイズできます。3つのフォーマットをサポート:

```toml
[keybindings]
# 単一キー
move_down = "j"

# 修飾子付きキー
page_down = { key = "d", ctrl = true }

# 2キーシーケンス
go_to_definition = ["g", "d"]
```

#### 設定可能なキー一覧

| キー | デフォルト | 説明 |
|-----|---------|-------------|
| **ナビゲーション** |||
| `move_down` | `j` | 下に移動 |
| `move_up` | `k` | 上に移動 |
| `move_left` | `h` | 左に移動 / 戻る |
| `move_right` | `l` | 右に移動 / 選択 |
| `page_down` | `Ctrl+d` | ページダウン |
| `page_up` | `Ctrl+u` | ページアップ |
| `jump_to_first` | `gg` | 先頭にジャンプ |
| `jump_to_last` | `G` | 末尾にジャンプ |
| `jump_back` | `Ctrl+o` | 前の位置に戻る |
| `next_comment` | `n` | 次のコメントにジャンプ |
| `prev_comment` | `N` | 前のコメントにジャンプ |
| **アクション** |||
| `approve` | `a` | PR を Approve |
| `request_changes` | `r` | Request changes |
| `comment` | `c` | コメント追加 |
| `suggestion` | `s` | サジェスチョン追加 |
| `reply` | `r` | コメントに返信 |
| `refresh` | `R` | 強制リフレッシュ |
| `submit` | `Ctrl+s` | 入力を送信 |
| **モード切替** |||
| `quit` | `q` | 終了 / 戻る |
| `help` | `?` | ヘルプを表示 |
| `comment_list` | `C` | コメント一覧を開く |
| `ai_rally` | `A` | AI Rally を開始 |
| `open_panel` | `Enter` | パネルを開く / 選択 |
| **Diff 操作** |||
| `go_to_definition` | `gd` | 定義へジャンプ |
| `go_to_file` | `gf` | $EDITOR でファイルを開く |

**Note**: 矢印キー（`↑/↓/←/→`）は常に Vim スタイルキーの代替として動作し、リマップできません。

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
│    Reviewer     │  AI が PR の diff をレビュー
│ (Claude/Codex)  │  → PR にレビューコメントを投稿
└────────┬────────┘
         │
    ┌────┴────┐
    │ Approve?│
    └────┬────┘
     No  │  Yes ──→ 完了 ✓
         ▼
┌─────────────────┐
│    Reviewee     │  AI が問題を修正
│ (Claude/Codex)  │  → ローカルにコミット（デフォルトで push なし）
└────────┬────────┘
         │
    ┌────┴──────────────┐
    │                   │
    ▼                   ▼
 完了          NeedsClarification /
    │           NeedsPermission
    │                   │
    │          ユーザーが応答 (y/n)
    │                   │
    └─────────┬─────────┘
              ▼
┌───────────────────────┐
│  再レビュー (Reviewer) │  更新された diff:
│                       │  git diff（ローカル優先）or
│                       │  gh pr diff（push 済みの場合）
└───────────┬───────────┘
            │
       ┌────┴────┐
       │ Approve?│  ... Approve または最大
       └─────────┘      イテレーションまで繰り返し
```

### 特徴

- **PR 統合**: レビューコメントは自動的に PR に投稿
- **外部 Bot サポート**: Copilot、CodeRabbit 等の Bot からのフィードバックを収集
- **安全な操作**: 危険な git 操作（`--force`、`reset --hard`）は禁止
- **セッション永続化**: Rally の状態はローカルに保存され、再開可能
- **インタラクティブフロー**: AI エージェントが確認や許可を求める際、対話的に応答可能
- **ローカル Diff サポート**: 再レビュー時はローカルの `git diff` を優先して未プッシュの変更を検出。push 済みの場合は `gh pr diff` にフォールバック
- **バックグラウンド実行**: `b` を押すと Rally をバックグラウンドで実行しながらファイル閲覧を継続可能

### 推奨構成

Codex はサンドボックスモードを使用し、細粒度のツール権限制御ができません。
最大限のセキュリティのため、以下の構成を推奨:

| 役割 | 推奨 | 理由 |
|------|-------------|--------|
| Reviewer | Codex または Claude | 読み取り専用操作のため、どちらも安全 |
| Reviewee | **Claude** | allowedTools による細粒度のツール制御が可能 |

安全な構成の例:

```toml
[ai]
reviewer = "codex"   # 安全: 読み取り専用サンドボックス
reviewee = "claude"  # 推奨: 細粒度のツール制御
reviewee_additional_tools = ["Skill"]  # 必要なものだけ追加
```

**注意**: Codex を reviewee として使用する場合、`--full-auto` モードで実行され、
ワークスペースへの書き込みアクセスとツール制限なしで動作します。

### ツール権限

#### デフォルトで許可されるツール

**Reviewer**（読み取り専用操作）:

| ツール | 説明 |
|------|-------------|
| Read, Glob, Grep | ファイル読み取りと検索 |
| `gh pr view/diff/checks` | PR 情報の表示 |
| `gh api --method GET` | GitHub API（GET のみ） |

**Reviewee**（コード修正）:

| カテゴリ | コマンド |
|----------|----------|
| ファイル | Read, Edit, Write, Glob, Grep |
| Git | status, diff, add, commit, log, show, branch, switch, stash |
| GitHub CLI | pr view, pr diff, pr checks, api GET |
| Cargo | build, test, check, clippy, fmt, run |
| npm/pnpm/bun | install, test, run |

#### 追加ツール（Claude only）

追加ツールは設定で有効化できます。Claude Code の `--allowedTools` 形式を使用:

| 例 | 説明 |
|---------|-------------|
| `"Skill"` | Claude Code スキルの実行 |
| `"WebFetch"` | URL コンテンツの取得 |
| `"WebSearch"` | Web 検索 |
| `"Bash(git push:*)"` | リモートへの git push |
| `"Bash(gh api --method POST:*)"` | GitHub API POST リクエスト |

```toml
[ai]
reviewee_additional_tools = ["Skill", "Bash(git push:*)"]
```

**Breaking Change (v0.2.0)**: `git push` はデフォルトで無効になりました。
有効にするには `"Bash(git push:*)"` を `reviewee_additional_tools` に追加してください。

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
