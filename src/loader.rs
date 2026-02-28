use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::warn;

use crate::diff;
use crate::github::{self, ChangedFile, PullRequest};

pub enum DataLoadResult {
    /// APIからデータ取得成功
    Success {
        pr: Box<PullRequest>,
        files: Vec<ChangedFile>,
    },
    /// エラー
    Error(String),
}

/// 単一ファイルの diff 結果（バッチ/オンデマンド共通）
pub struct SingleFileDiffResult {
    pub filename: String,
    pub patch: Option<String>,
}

/// コメント送信結果
pub enum CommentSubmitResult {
    /// 送信成功
    Success,
    /// エラー
    Error(String),
}

/// バックグラウンド取得モード
pub enum FetchMode {
    /// 新規取得（キャッシュミスまたは強制更新）
    Fresh,
    /// 更新チェックのみ（キャッシュヒット時）
    CheckUpdate(String), // cached updated_at
}

/// バックグラウンドでPRデータを取得
pub async fn fetch_pr_data(
    repo: String,
    pr_number: u32,
    mode: FetchMode,
    tx: mpsc::Sender<DataLoadResult>,
) {
    match mode {
        FetchMode::Fresh => {
            fetch_and_send(&repo, pr_number, tx).await;
        }
        FetchMode::CheckUpdate(cached_updated_at) => {
            check_for_updates(&repo, pr_number, &cached_updated_at, tx).await;
        }
    }
}

/// ローカル `git diff` から PR データを再構築して読み込み（2段階ロード版）
///
/// Phase 1: name-status + numstat のみ → ファイル一覧（patch: None）を即座に送信
/// Phase 2: バッチ diff ロードは app.rs 側で start_batch_diff_loading() 経由で行う
pub async fn fetch_local_diff(
    _repo: String,
    working_dir: Option<String>,
    tx: mpsc::Sender<DataLoadResult>,
) {
    let current_workdir = working_dir.as_deref();

    // 1. name-status（ファイル名 + ステータス）— 高速
    let name_status_output = match run_git_name_status(current_workdir).await {
        Ok(output) => output,
        Err(e) => {
            let _ = tx.send(DataLoadResult::Error(e.to_string())).await;
            return;
        }
    };
    let file_statuses = parse_name_status_output(&name_status_output);

    // 2. numstat（additions/deletions）— 高速
    let numstat_output = run_git_numstat(current_workdir).await.ok();
    let file_changes = parse_numstat_output(numstat_output.as_deref());

    // 3. ChangedFile を patch: None で構築
    let mut files = build_changed_files_lazy(&file_statuses, &file_changes);

    // 4. untracked ファイルもリストのみ取得（patch: None）
    merge_untracked_files_lazy(current_workdir, &mut files).await;

    // 5. PR情報を構築して即座に送信
    let pr = PullRequest {
        number: 0,
        node_id: None,
        title: "Local HEAD diff".to_string(),
        body: Some("Current working tree diff from HEAD".to_string()),
        state: "local".to_string(),
        head: github::Branch {
            ref_name: "HEAD".to_string(),
            sha: current_head_sha(current_workdir)
                .await
                .unwrap_or_else(|_| "local".to_string()),
        },
        base: github::Branch {
            ref_name: "local".to_string(),
            sha: "local".to_string(),
        },
        user: github::User {
            login: "local".to_string(),
        },
        updated_at: Utc::now().to_rfc3339(),
    };

    let _ = tx
        .send(DataLoadResult::Success {
            pr: Box::new(pr),
            files,
        })
        .await;
}

/// バッチ diff ロード: ファイルリスト順にバッチで diff を取得し、チャネルに送信
pub async fn fetch_local_diffs_batched(
    working_dir: Option<String>,
    filenames: Vec<String>,
    untracked_filenames: Vec<String>,
    batch_size: usize,
    tx: mpsc::Sender<Vec<SingleFileDiffResult>>,
) {
    let wd = working_dir.as_deref();

    // tracked ファイルをバッチで処理
    for batch in filenames.chunks(batch_size) {
        let mut args = vec!["diff", "HEAD", "--"];
        let batch_strs: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();
        args.extend(&batch_strs);

        let output = run_git_command(wd, &args).await;
        let mut patches = match output {
            Ok(diff_output) => diff::parse_unified_diff(&diff_output),
            Err(_) => HashMap::new(),
        };

        let results: Vec<SingleFileDiffResult> = batch
            .iter()
            .map(|filename| SingleFileDiffResult {
                filename: filename.clone(),
                patch: patches.remove(filename),
            })
            .collect();

        if tx.send(results).await.is_err() {
            return;
        }
    }

    // untracked ファイルは per-file で git diff --no-index（バッチ不可）
    for batch in untracked_filenames.chunks(batch_size) {
        let mut results = Vec::with_capacity(batch.len());
        for filename in batch {
            let patch = run_git_no_index_diff(wd, filename).await.ok();
            let patch = patch.filter(|p| !p.is_empty());
            results.push(SingleFileDiffResult {
                filename: filename.clone(),
                patch,
            });
        }
        if tx.send(results).await.is_err() {
            return;
        }
    }
}

/// 単一ファイルの diff をオンデマンド取得（tracked + untracked 自動判別）
pub async fn fetch_single_file_diff(
    working_dir: Option<String>,
    filename: String,
    is_untracked: bool,
    tx: mpsc::Sender<SingleFileDiffResult>,
) {
    let wd = working_dir.as_deref();

    let patch = if is_untracked {
        run_git_no_index_diff(wd, &filename)
            .await
            .ok()
            .filter(|p| !p.is_empty())
    } else {
        run_git_diff_file(wd, &filename)
            .await
            .ok()
            .filter(|p| !p.is_empty())
    };

    let _ = tx.send(SingleFileDiffResult { filename, patch }).await;
}

async fn fetch_and_send(repo: &str, pr_number: u32, tx: mpsc::Sender<DataLoadResult>) {
    match tokio::try_join!(
        github::fetch_pr(repo, pr_number),
        github::fetch_changed_files(repo, pr_number)
    ) {
        Ok((pr, mut files)) => {
            if let Some(pr_node_id) = pr.node_id.as_deref() {
                match github::fetch_files_viewed_state(repo, pr_node_id).await {
                    Ok(viewed_state) => {
                        for file in files.iter_mut() {
                            file.viewed =
                                viewed_state.get(&file.filename).copied().unwrap_or(false);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to fetch viewed-state for PR files: {}", e);
                    }
                }
            }

            // Check if any files have missing patches (large file limitation)
            let has_missing_patches = files.iter().any(|f| f.patch.is_none());

            if has_missing_patches {
                // Fetch full diff using gh pr diff as fallback
                match github::fetch_pr_diff(repo, pr_number).await {
                    Ok(full_diff) => {
                        let mut patch_map = diff::parse_unified_diff(&full_diff);

                        // Apply patches only to files that are missing them
                        for file in files.iter_mut() {
                            if file.patch.is_none() {
                                if let Some(patch) = patch_map.remove(&file.filename) {
                                    file.patch = Some(patch);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Fallback failed, log warning and continue with "No diff available"
                        warn!("Failed to fetch full diff for fallback: {}", e);
                    }
                }
            }

            let _ = tx
                .send(DataLoadResult::Success {
                    pr: Box::new(pr),
                    files,
                })
                .await;
        }
        Err(e) => {
            let _ = tx.send(DataLoadResult::Error(e.to_string())).await;
        }
    }
}

async fn check_for_updates(
    repo: &str,
    pr_number: u32,
    cached_updated_at: &str,
    tx: mpsc::Sender<DataLoadResult>,
) {
    // PRの基本情報だけ取得してupdated_atを比較
    if let Ok(fresh_pr) = github::fetch_pr(repo, pr_number).await {
        if fresh_pr.updated_at != cached_updated_at {
            // 更新あり → 全データ再取得
            fetch_and_send(repo, pr_number, tx).await;
        }
        // 更新なし → 何もしない（キャッシュデータをそのまま使用）
    }
}

async fn run_git_numstat(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["diff", "--numstat", "HEAD"]).await
}

async fn run_git_name_status(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["diff", "--name-status", "HEAD"]).await
}

/// Git の C-quoted パス文字列をデコードする。
///
/// `core.quotePath` が true（デフォルト）の場合、非 ASCII パスは
/// `"src/\343\201\202.rs"` のように C-quoted 形式で出力される。
/// この関数は引用符を除去し、オクタルエスケープを UTF-8 バイト列に変換する。
///
/// 引用符で囲まれていないパスはそのまま返す。
fn unquote_git_path(path: &str) -> String {
    // C-quoted 形式でなければそのまま返す
    if !path.starts_with('"') || !path.ends_with('"') {
        return path.to_string();
    }

    // 前後の引用符を除去
    let inner = &path[1..path.len() - 1];
    let mut bytes: Vec<u8> = Vec::with_capacity(inner.len());
    let mut chars = inner.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('\\') => bytes.push(b'\\'),
                Some('"') => bytes.push(b'"'),
                Some('n') => bytes.push(b'\n'),
                Some('t') => bytes.push(b'\t'),
                Some('a') => bytes.push(0x07), // bell
                Some('b') => bytes.push(0x08), // backspace
                Some('r') => bytes.push(b'\r'),
                Some('f') => bytes.push(0x0C), // form feed
                Some('v') => bytes.push(0x0B), // vertical tab
                Some(c) if c.is_ascii_digit() && c != '8' && c != '9' => {
                    // Octal escape: \ooo (1-3 digits)
                    let mut octal = String::with_capacity(3);
                    octal.push(c);
                    // Peek at next chars for octal digits
                    for _ in 0..2 {
                        // We need to peek, but chars doesn't support peek.
                        // Use a clone trick.
                        let mut peek = chars.clone();
                        if let Some(next) = peek.next() {
                            if next.is_ascii_digit() && next != '8' && next != '9' {
                                octal.push(next);
                                chars.next(); // consume
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    if let Ok(byte) = u8::from_str_radix(&octal, 8) {
                        bytes.push(byte);
                    }
                }
                Some(c) => {
                    // Unknown escape, keep as-is
                    bytes.push(b'\\');
                    let mut buf = [0u8; 4];
                    bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                None => bytes.push(b'\\'),
            }
        } else {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }

    String::from_utf8(bytes).unwrap_or_else(|e| {
        // Fallback: lossy conversion
        String::from_utf8_lossy(e.as_bytes()).into_owned()
    })
}

/// name-status 出力をパース。rename/copy の3カラム形式にも対応:
///   "M\tsrc/foo.rs"           → ("src/foo.rs", "modified")
///   "A\tsrc/new.rs"           → ("src/new.rs", "added")
///   "D\tsrc/old.rs"           → ("src/old.rs", "removed")
///   "R100\told.rs\tnew.rs"    → ("new.rs", "renamed")
///   "C100\tsrc.rs\tdst.rs"    → ("dst.rs", "copied")
///
/// Git の C-quoted パス（非 ASCII ファイル名）も自動でデコードする。
fn parse_name_status_output(output: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }
        let status_code = parts[0];
        let first_char = status_code.chars().next().unwrap_or(' ');

        match first_char {
            'R' | 'C' => {
                // Rename/Copy: 3カラム形式 "R100\told\tnew" or "C100\tsrc\tdst"
                if parts.len() >= 3 {
                    let new_name = unquote_git_path(parts[2]);
                    let status = if first_char == 'R' {
                        "renamed"
                    } else {
                        "copied"
                    };
                    result.push((new_name, status.to_string()));
                }
            }
            'M' => {
                result.push((unquote_git_path(parts[1]), "modified".to_string()));
            }
            'A' => {
                result.push((unquote_git_path(parts[1]), "added".to_string()));
            }
            'D' => {
                result.push((unquote_git_path(parts[1]), "removed".to_string()));
            }
            'T' => {
                // Type change
                result.push((unquote_git_path(parts[1]), "modified".to_string()));
            }
            _ => {
                // Unknown status, treat as modified
                result.push((unquote_git_path(parts[1]), "modified".to_string()));
            }
        }
    }

    result
}

/// name-status + numstat から ChangedFile（patch: None）を構築
fn build_changed_files_lazy(
    name_status: &[(String, String)],
    numstat: &HashMap<String, (u32, u32)>,
) -> Vec<ChangedFile> {
    let mut files: Vec<ChangedFile> = name_status
        .iter()
        .map(|(filename, status)| {
            let (additions, deletions) = numstat.get(filename).copied().unwrap_or((0, 0));
            ChangedFile {
                filename: filename.clone(),
                status: status.clone(),
                additions,
                deletions,
                patch: None,
                viewed: false,
            }
        })
        .collect();

    files.sort_unstable_by(|a, b| a.filename.cmp(&b.filename));
    files
}

/// untracked ファイルをリストのみ取得（patch: None）
async fn merge_untracked_files_lazy(working_dir: Option<&str>, files: &mut Vec<ChangedFile>) {
    let untracked_output = match run_git_untracked(working_dir).await {
        Ok(output) => output,
        Err(_) => return,
    };
    let names = parse_path_list(&untracked_output);

    for filename in names {
        if files.iter().any(|f| f.filename == filename) {
            continue;
        }

        files.push(ChangedFile {
            filename,
            status: "added".to_string(),
            additions: 0,
            deletions: 0,
            patch: None,
            viewed: false,
        });
    }

    files.sort_unstable_by(|a, b| a.filename.cmp(&b.filename));
}

async fn current_head_sha(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["rev-parse", "HEAD"])
        .await
        .map(|s| s.trim().to_string())
}

async fn run_git_command(working_dir: Option<&str>, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    // Disable C-quoting of non-ASCII paths to get raw UTF-8 output
    command.args(["-c", "core.quotePath=false"]);
    command.args(args);

    if let Some(dir) = working_dir {
        command.current_dir(dir);
    }

    let output = command
        .output()
        .await
        .context("failed to spawn git command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_numstat_output(output: Option<&str>) -> HashMap<String, (u32, u32)> {
    let mut result = HashMap::new();
    let Some(output) = output else {
        return result;
    };

    for line in output.lines() {
        let mut parts = line.split('\t');
        let added_raw = parts.next();
        let deleted_raw = parts.next();
        let path = parts.next_back();

        if let (Some(added_raw), Some(deleted_raw), Some(path)) = (added_raw, deleted_raw, path) {
            let parse_count = |value: &str| -> u32 { value.parse().unwrap_or(0) };
            result.insert(
                unquote_git_path(path),
                (parse_count(added_raw), parse_count(deleted_raw)),
            );
        }
    }

    result
}

fn parse_path_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|line| unquote_git_path(line.trim()))
        .filter(|line| !line.is_empty())
        .collect()
}

async fn run_git_diff_file(working_dir: Option<&str>, filename: &str) -> Result<String> {
    run_git_command(working_dir, &["diff", "HEAD", "--", filename]).await
}

async fn run_git_untracked(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["ls-files", "--others", "--exclude-standard"]).await
}

async fn run_git_no_index_diff(working_dir: Option<&str>, filename: &str) -> Result<String> {
    let mut command = Command::new("git");
    // Disable C-quoting of non-ASCII paths to get raw UTF-8 output
    command.args(["-c", "core.quotePath=false"]);
    command.args([
        "diff",
        "--no-ext-diff",
        "--no-color",
        "--no-index",
        "--",
        "/dev/null",
        filename,
    ]);

    if let Some(dir) = working_dir {
        command.current_dir(dir);
    }

    let output = command
        .output()
        .await
        .context("failed to spawn git no-index diff command")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() {
        return Ok(stdout);
    }

    if stdout.trim().is_empty() && !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        anyhow::bail!("git diff --no-index failed: {}", stderr.trim());
    }

    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    fn run_git(cmd: &mut Command, dir: &Path, args: &[&str], message: &str) {
        let status = cmd
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "octorus-test")
            .env("GIT_AUTHOR_EMAIL", "octorus-test@example.com")
            .env("GIT_COMMITTER_NAME", "octorus-test")
            .env("GIT_COMMITTER_EMAIL", "octorus-test@example.com")
            .status()
            .expect(message);

        assert!(status.success(), "{message}: {status}");
    }

    fn write_file(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[tokio::test]
    async fn test_fetch_local_diff_detects_subdir_changes_and_skips_ignored_files() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join(".gitignore"), "ignored/\n");
        write_file(&workdir.join("src/main.rs"), "fn main() {}\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", ".gitignore", "src/main.rs"],
            "failed to add initial files",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to create initial commit",
        );

        write_file(
            &workdir.join("src/main.rs"),
            "fn main() { println!(\"hello\"); }\n",
        );
        write_file(
            &workdir.join("feature/new_file.rs"),
            "pub fn feature() {}\n",
        );
        write_file(
            &workdir.join("ignored/skip.txt"),
            "this file should stay ignored",
        );

        let (tx, mut rx) = mpsc::channel::<DataLoadResult>(1);
        fetch_local_diff(
            "local".to_string(),
            Some(workdir.to_string_lossy().to_string()),
            tx,
        )
        .await;

        let result = rx.recv().await.unwrap();
        let files = match result {
            DataLoadResult::Success { files, .. } => files,
            DataLoadResult::Error(err) => panic!("unexpected error: {err}"),
        };

        let filenames: Vec<_> = files.iter().map(|file| file.filename.as_str()).collect();

        assert!(filenames.contains(&"src/main.rs"));
        assert!(filenames.contains(&"feature/new_file.rs"));
        assert!(!filenames.contains(&"ignored/skip.txt"));
    }

    #[tokio::test]
    async fn test_fetch_local_diff_does_not_return_non_target_ignored_file() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join(".gitignore"), "ignored/\n");
        write_file(&workdir.join("README.md"), "# octorus\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", ".gitignore", "README.md"],
            "failed to add initial files",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to create initial commit",
        );

        write_file(&workdir.join("ignored/build.tmp"), "ignore\n");

        let (tx, mut rx) = mpsc::channel::<DataLoadResult>(1);
        fetch_local_diff(
            "local".to_string(),
            Some(workdir.to_string_lossy().to_string()),
            tx,
        )
        .await;

        let result = rx.recv().await.unwrap();
        let files = match result {
            DataLoadResult::Success { files, .. } => files,
            DataLoadResult::Error(err) => panic!("unexpected error: {err}"),
        };

        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_local_diff_for_untracked_file_returns_lazy() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join("README.md"), "hello\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "README.md"],
            "failed to add initial file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to commit initial file",
        );

        write_file(
            &workdir.join("src/new_feature.rs"),
            "pub fn hello() {\n    1 + 1\n}\n",
        );

        let (tx, mut rx) = mpsc::channel::<DataLoadResult>(1);
        fetch_local_diff(
            "local".to_string(),
            Some(workdir.to_string_lossy().to_string()),
            tx,
        )
        .await;

        let result = rx.recv().await.unwrap();
        let files = match result {
            DataLoadResult::Success { files, .. } => files,
            DataLoadResult::Error(err) => panic!("unexpected error: {err}"),
        };

        let new_file = files
            .iter()
            .find(|file| file.filename == "src/new_feature.rs")
            .expect("untracked file should appear in local diff");

        // 2段階ロードでは patch は None（バッチロードで後から取得）
        assert!(new_file.patch.is_none());
        assert_eq!(new_file.status, "added");
    }

    #[tokio::test]
    async fn test_fetch_single_file_diff_for_tracked_file() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join("src/main.rs"), "fn main() {}\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "src/main.rs"],
            "failed to add initial file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to create initial commit",
        );

        write_file(
            &workdir.join("src/main.rs"),
            "fn main() { println!(\"hello\"); }\n",
        );

        let (tx, mut rx) = mpsc::channel::<SingleFileDiffResult>(1);
        fetch_single_file_diff(
            Some(workdir.to_string_lossy().to_string()),
            "src/main.rs".to_string(),
            false,
            tx,
        )
        .await;

        let result = rx.recv().await.unwrap();
        assert_eq!(result.filename, "src/main.rs");
        let patch = result.patch.expect("tracked file should have a patch");
        assert!(patch.contains("+fn main() { println!(\"hello\"); }"));
    }

    #[tokio::test]
    async fn test_fetch_single_file_diff_for_untracked_file() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join("README.md"), "hello\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "README.md"],
            "failed to add initial file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to create initial commit",
        );

        write_file(
            &workdir.join("src/new_feature.rs"),
            "pub fn hello() {\n    1 + 1\n}\n",
        );

        let (tx, mut rx) = mpsc::channel::<SingleFileDiffResult>(1);
        fetch_single_file_diff(
            Some(workdir.to_string_lossy().to_string()),
            "src/new_feature.rs".to_string(),
            true,
            tx,
        )
        .await;

        let result = rx.recv().await.unwrap();
        assert_eq!(result.filename, "src/new_feature.rs");
        let patch = result.patch.expect("untracked file should have a patch");
        assert!(patch.contains("+pub fn hello()"));
    }

    #[test]
    fn test_parse_name_status_output() {
        let output = "M\tsrc/foo.rs\nA\tsrc/new.rs\nD\tsrc/old.rs\nR100\told.rs\tnew.rs\nC100\tsrc.rs\tdst.rs\n";
        let result = parse_name_status_output(output);

        assert_eq!(result.len(), 5);
        assert_eq!(
            result[0],
            ("src/foo.rs".to_string(), "modified".to_string())
        );
        assert_eq!(result[1], ("src/new.rs".to_string(), "added".to_string()));
        assert_eq!(result[2], ("src/old.rs".to_string(), "removed".to_string()));
        assert_eq!(result[3], ("new.rs".to_string(), "renamed".to_string()));
        assert_eq!(result[4], ("dst.rs".to_string(), "copied".to_string()));
    }

    #[test]
    fn test_parse_name_status_output_empty() {
        let result = parse_name_status_output("");
        assert!(result.is_empty());
    }

    /// parse_unified_diff のキーと parse_name_status_output のファイル名が
    /// rename ファイルでも一致することを検証する。
    /// これが失敗すると fetch_local_diffs_batched の patches.remove(filename) が
    /// None を返し、rename ファイルの diff が取得できなくなる。
    #[test]
    fn test_parse_unified_diff_filename_matches_name_status_for_rename() {
        let name_status_output = "R100\tsrc/old_name.rs\tsrc/new_name.rs\n";
        let name_status = parse_name_status_output(name_status_output);
        assert_eq!(name_status[0].0, "src/new_name.rs");

        let unified_diff = "\
diff --git a/src/old_name.rs b/src/new_name.rs
similarity index 95%
rename from src/old_name.rs
rename to src/new_name.rs
index 1234567..abcdefg 100644
--- a/src/old_name.rs
+++ b/src/new_name.rs
@@ -1,3 +1,3 @@
-fn old_name() {
+fn new_name() {
 }";
        let mut patches = diff::parse_unified_diff(unified_diff);

        // name_status のファイル名で patch を取得できること
        let patch = patches.remove(&name_status[0].0);
        assert!(
            patch.is_some(),
            "parse_unified_diff key must match parse_name_status_output filename for renamed files"
        );
    }

    /// parse_unified_diff のキーと parse_name_status_output のファイル名が
    /// 通常の変更ファイルで一致することを検証する。
    #[test]
    fn test_parse_unified_diff_filename_matches_name_status_for_modified() {
        let name_status_output = "M\tsrc/main.rs\n";
        let name_status = parse_name_status_output(name_status_output);
        assert_eq!(name_status[0].0, "src/main.rs");

        let unified_diff = "\
diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!(\"Hello\");
 }";
        let mut patches = diff::parse_unified_diff(unified_diff);

        let patch = patches.remove(&name_status[0].0);
        assert!(
            patch.is_some(),
            "parse_unified_diff key must match parse_name_status_output filename for modified files"
        );
    }

    #[tokio::test]
    async fn test_fetch_local_diffs_batched_handles_renamed_file() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join("src/old_name.rs"), "fn old_name() {}\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "src/old_name.rs"],
            "failed to add initial file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to create initial commit",
        );

        // git mv でリネーム + 内容変更
        run_git(
            &mut Command::new("git"),
            workdir,
            &["mv", "src/old_name.rs", "src/new_name.rs"],
            "failed to rename file",
        );
        write_file(&workdir.join("src/new_name.rs"), "fn new_name() {}\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "src/new_name.rs"],
            "failed to stage renamed file",
        );

        // バッチ diff で新ファイル名を使って patch が取得できることを検証
        let (tx, mut rx) = mpsc::channel::<Vec<SingleFileDiffResult>>(2);
        fetch_local_diffs_batched(
            Some(workdir.to_string_lossy().to_string()),
            vec!["src/new_name.rs".to_string()],
            vec![],
            20,
            tx,
        )
        .await;

        let results = rx.recv().await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "src/new_name.rs");
        assert!(
            results[0].patch.is_some(),
            "renamed file must have a patch when queried with new filename"
        );
    }

    #[test]
    fn test_build_changed_files_lazy() {
        let name_status = vec![
            ("src/foo.rs".to_string(), "modified".to_string()),
            ("src/new.rs".to_string(), "added".to_string()),
        ];
        let mut numstat = HashMap::new();
        numstat.insert("src/foo.rs".to_string(), (3u32, 1u32));
        numstat.insert("src/new.rs".to_string(), (10u32, 0u32));

        let files = build_changed_files_lazy(&name_status, &numstat);

        assert_eq!(files.len(), 2);
        // ソートされるので foo が先
        assert_eq!(files[0].filename, "src/foo.rs");
        assert_eq!(files[0].status, "modified");
        assert_eq!(files[0].additions, 3);
        assert_eq!(files[0].deletions, 1);
        assert!(files[0].patch.is_none());

        assert_eq!(files[1].filename, "src/new.rs");
        assert_eq!(files[1].status, "added");
        assert_eq!(files[1].additions, 10);
        assert_eq!(files[1].deletions, 0);
        assert!(files[1].patch.is_none());
    }

    #[test]
    fn test_unquote_git_path_plain() {
        assert_eq!(unquote_git_path("src/foo.rs"), "src/foo.rs");
    }

    #[test]
    fn test_unquote_git_path_cquoted_non_ascii() {
        // \343\201\202 = UTF-8 encoding of 'あ' (U+3042)
        assert_eq!(unquote_git_path(r#""src/\343\201\202.rs""#), "src/あ.rs");
    }

    #[test]
    fn test_unquote_git_path_cquoted_backslash_and_quote() {
        assert_eq!(unquote_git_path(r#""path\\to\"file""#), "path\\to\"file");
    }

    #[test]
    fn test_unquote_git_path_cquoted_special_escapes() {
        assert_eq!(unquote_git_path(r#""a\tb\nc""#), "a\tb\nc");
    }

    #[test]
    fn test_unquote_git_path_cquoted_multibyte_sequence() {
        // \346\227\245\346\234\254\350\252\236 = UTF-8 encoding of '日本語'
        assert_eq!(
            unquote_git_path(r#""src/\346\227\245\346\234\254\350\252\236.txt""#),
            "src/日本語.txt"
        );
    }

    #[test]
    fn test_parse_name_status_output_cquoted_paths() {
        // Git C-quotes non-ASCII filenames by default (core.quotePath=true)
        // In real git output, the tab-separated line looks like:
        //   M\t"src/\343\201\202.rs"
        // We construct this with format! to avoid Rust escape conflicts
        let output = format!("M\t{}\n", r#""src/\343\201\202.rs""#);
        let result = parse_name_status_output(&output);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "src/あ.rs");
        assert_eq!(result[0].1, "modified");
    }

    #[test]
    fn test_parse_numstat_output_cquoted_paths() {
        let output = format!("3\t1\t{}\n", r#""src/\343\201\202.rs""#);
        let result = parse_numstat_output(Some(&output));

        assert_eq!(result.len(), 1);
        assert!(
            result.contains_key("src/あ.rs"),
            "numstat should decode quoted paths"
        );
        assert_eq!(result["src/あ.rs"], (3, 1));
    }

    #[test]
    fn test_parse_path_list_cquoted_paths() {
        let output = format!("{}\nsrc/plain.rs\n", r#""src/\343\201\202.rs""#);
        let result = parse_path_list(&output);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "src/あ.rs");
        assert_eq!(result[1], "src/plain.rs");
    }

    #[tokio::test]
    async fn test_fetch_local_diffs_batched_non_ascii_filename() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join("README.md"), "hello\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "README.md"],
            "failed to add initial file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to commit initial file",
        );

        // Create a non-ASCII tracked file, commit, then modify
        let non_ascii_path = workdir.join("src/日本語.rs");
        write_file(&non_ascii_path, "fn original() {}\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "src/日本語.rs"],
            "failed to add non-ASCII file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "add non-ASCII file"],
            "failed to commit non-ASCII file",
        );

        // Modify the file so it appears in diff
        write_file(&non_ascii_path, "fn modified() {}\n");

        // First, verify fetch_local_diff returns the decoded filename
        let (tx, mut rx) = mpsc::channel::<DataLoadResult>(1);
        fetch_local_diff(
            "local".to_string(),
            Some(workdir.to_string_lossy().to_string()),
            tx,
        )
        .await;

        let result = rx.recv().await.unwrap();
        let files = match result {
            DataLoadResult::Success { files, .. } => files,
            DataLoadResult::Error(err) => panic!("unexpected error: {err}"),
        };

        let non_ascii_file = files
            .iter()
            .find(|f| f.filename == "src/日本語.rs")
            .expect("non-ASCII filename should be decoded from C-quoted format");
        assert_eq!(non_ascii_file.status, "modified");

        // Now verify batched diff can retrieve the patch using decoded filename
        let (tx2, mut rx2) = mpsc::channel::<Vec<SingleFileDiffResult>>(2);
        fetch_local_diffs_batched(
            Some(workdir.to_string_lossy().to_string()),
            vec!["src/日本語.rs".to_string()],
            vec![],
            20,
            tx2,
        )
        .await;

        let results = rx2.recv().await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "src/日本語.rs");
        assert!(
            results[0].patch.is_some(),
            "batched diff must retrieve patch for non-ASCII filename"
        );
    }

    #[tokio::test]
    async fn test_fetch_single_file_diff_non_ascii_filename() {
        let tempdir = tempdir().unwrap();
        let workdir = tempdir.path();

        run_git(
            &mut Command::new("git"),
            workdir,
            &["init", "-b", "main"],
            "failed to initialize temp git repo",
        );
        write_file(&workdir.join("README.md"), "hello\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "README.md"],
            "failed to add initial file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "initial commit"],
            "failed to commit initial file",
        );

        // Create and commit a non-ASCII tracked file, then modify
        let non_ascii_path = workdir.join("src/テスト.rs");
        write_file(&non_ascii_path, "fn original() {}\n");
        run_git(
            &mut Command::new("git"),
            workdir,
            &["add", "src/テスト.rs"],
            "failed to add non-ASCII file",
        );
        run_git(
            &mut Command::new("git"),
            workdir,
            &["commit", "-m", "add non-ASCII file"],
            "failed to commit non-ASCII file",
        );

        write_file(&non_ascii_path, "fn modified() {}\n");

        let (tx, mut rx) = mpsc::channel::<SingleFileDiffResult>(1);
        fetch_single_file_diff(
            Some(workdir.to_string_lossy().to_string()),
            "src/テスト.rs".to_string(),
            false,
            tx,
        )
        .await;

        let result = rx.recv().await.unwrap();
        assert_eq!(result.filename, "src/テスト.rs");
        assert!(
            result.patch.is_some(),
            "single file diff must retrieve patch for non-ASCII filename"
        );
        let patch = result.patch.unwrap();
        assert!(patch.contains("+fn modified()"));
    }
}
