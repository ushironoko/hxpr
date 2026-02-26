---
name: cargo-release
description: Rust/Cargoプロジェクトのバージョン更新、GitHubリリース、crates.io publishを実行する。
---

# Cargo Release Workflow

Rust/Cargoプロジェクトのリリース手順を実行するスキル。

## 引数

| 引数 | 必須 | 説明 |
|------|------|------|
| version | Yes | リリースするバージョン（例: 0.2.0） |

## 使用例

```
/cargo-release 0.2.0
```

## 実行フロー

### Phase 1: 事前確認

1. **引数検証**
   - バージョン引数が指定されていることを確認
   - semver形式（X.Y.Z）であることを確認

2. **現在のバージョン確認**
   ```bash
   grep '^version' Cargo.toml
   ```

3. **git状態確認**
   ```bash
   git status --porcelain
   ```
   - ワーキングディレクトリがクリーンでない場合は警告して中断

4. **既存タグ一覧確認**
   ```bash
   git tag --list 'v*' --sort=-version:refname | head -10
   ```
   - 同じバージョンのタグが既に存在する場合は中断

### Phase 2: バージョン更新

1. **Cargo.tomlのバージョン更新**
   ```bash
   # Cargo.toml の version = "X.Y.Z" を更新
   ```

2. **Cargo.lockの更新**
   ```bash
   cargo check
   ```

3. **テスト実行**
   ```bash
   cargo test
   ```
   - 失敗した場合は中断

### Phase 3: コミット & プッシュ

1. **変更をコミット**
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "chore: bump version to {version}"
   ```

2. **mainブランチにプッシュ**
   ```bash
   git push origin main
   ```

### Phase 4: リリースワークフロー起動

1. **バンプコミットのSHA取得**
   ```bash
   BUMP_SHA=$(git rev-parse HEAD)
   ```

2. **ワークフロー起動**
   ```bash
   gh workflow run release.yml -f version={version} -f bump_sha=$BUMP_SHA
   ```

3. **進捗確認用コマンドを表示**
   ```
   gh run list --workflow=release.yml --limit 1
   gh run watch <run-id>
   ```
   - ワークフローがtest/build/release/publishを自動実行
   - 失敗時はバージョンバンプコミットが自動revertされる

## 前提条件

- `gh` CLIがインストール・認証済み
- mainブランチにいること
- ワーキングディレクトリがクリーンであること
- リポジトリに `CARGO_REGISTRY_TOKEN` シークレットが設定済み（crates.io publish用）

## 注意事項

- ワークフローが失敗した場合、バンプコミットは自動revertされる（ただしHEADが移動していない場合のみ）
- `cargo publish` の失敗ではrollbackされない（GitHub Releaseは成功済みのため）

## エラー時のリカバリ

### validate/test/build/release の失敗
- 自動rollback（バンプコミットrevert + タグ/リリース削除）が実行される
- 原因を修正後、Phase 2 からやり直す（バージョン更新 → コミット → プッシュ → ワークフロー起動）

### publish のみ失敗（GitHub Release は成功済み）
- **推奨**: GitHub Actions UI から "Re-run failed jobs" で `publish` ジョブのみ再実行
- **手動**: ローカルで `cargo publish -p octorus` を実行（`CARGO_REGISTRY_TOKEN` が必要）
- **注意**: `gh workflow run` での再実行はタグ重複バリデーションで失敗するため使用不可

### ネットワークエラー（ワークフロー起動自体の失敗）
- `gh workflow run release.yml -f version={version} -f bump_sha=$BUMP_SHA` で再試行
