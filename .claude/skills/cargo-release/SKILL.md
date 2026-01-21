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

4. **リリースビルド確認**
   ```bash
   cargo build --release
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

### Phase 4: GitHubリリース

1. **前回タグから変更履歴を取得**
   ```bash
   PREV_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
   if [ -n "$PREV_TAG" ]; then
     git log ${PREV_TAG}..HEAD --oneline
   fi
   ```

2. **リリース作成**
   ```bash
   gh release create v{version} \
     --title "v{version}" \
     --generate-notes
   ```
   - `--generate-notes`: 前回リリースからの変更を自動生成

### Phase 5: crates.io Publish

1. **Dry-run検証**
   ```bash
   cargo publish --dry-run
   ```
   - 問題がある場合は警告を表示

2. **公開実行**
   ```bash
   cargo publish
   ```

## 前提条件

- `gh` CLIがインストール・認証済み
- `cargo login` でcrates.io認証済み
- mainブランチにいること
- ワーキングディレクトリがクリーンであること

## 注意事項

- crates.io publishは取り消せないため、Phase 5の前に最終確認を行う
- ネットワークエラーが発生した場合は手動で再試行が必要
