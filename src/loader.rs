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

/// ローカル `git diff` から PR データを再構築して読み込み
pub async fn fetch_local_diff(
    _repo: String,
    working_dir: Option<String>,
    tx: mpsc::Sender<DataLoadResult>,
) {
    let current_workdir = working_dir.as_deref();

    let diff_output = match run_git_diff(current_workdir).await {
        Ok(output) => output,
        Err(e) => {
            let _ = tx.send(DataLoadResult::Error(e.to_string())).await;
            return;
        }
    };

    let numstat_output = run_git_numstat(current_workdir).await.ok();
    let file_changes = parse_numstat_output(numstat_output.as_deref());

    let mut patches = diff::parse_unified_diff(&diff_output);
    let mut files = build_changed_files(&mut patches, &file_changes);

    if files.is_empty() || files.len() < file_changes.len() {
        merge_missing_local_changes(current_workdir, &file_changes, &mut files).await;
    }

    if files.is_empty() {
        merge_name_only_files(current_workdir, &mut files).await;
    }

    merge_untracked_files(current_workdir, &mut files).await;

    let pr = PullRequest {
        number: 0,
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

async fn merge_missing_local_changes(
    working_dir: Option<&str>,
    file_changes: &HashMap<String, (u32, u32)>,
    files: &mut Vec<ChangedFile>,
) {
    for (filename, (additions, deletions)) in file_changes {
        if files.iter().any(|f| f.filename == *filename) {
            continue;
        }

        let patch = run_git_diff_file(working_dir, filename)
            .await
            .unwrap_or_default();
        files.push(ChangedFile {
            filename: filename.clone(),
            status: status_from_patch(&patch)
                .unwrap_or_else(|| infer_status(*additions, *deletions)),
            additions: *additions,
            deletions: *deletions,
            patch: if patch.is_empty() { None } else { Some(patch) },
        });
    }

    files.sort_unstable_by(|a, b| a.filename.cmp(&b.filename));
}

async fn merge_name_only_files(working_dir: Option<&str>, files: &mut Vec<ChangedFile>) {
    let name_only_output = match run_git_name_only(working_dir).await {
        Ok(output) => output,
        Err(_) => return,
    };
    let names = parse_path_list(&name_only_output);

    for filename in names {
        if files.iter().any(|f| f.filename == filename) {
            continue;
        }

        let patch = run_git_diff_file(working_dir, &filename)
            .await
            .unwrap_or_default();
        files.push(ChangedFile {
            filename: filename.clone(),
            status: status_from_patch(&patch).unwrap_or_else(|| "modified".to_string()),
            additions: 0,
            deletions: 0,
            patch: if patch.is_empty() { None } else { Some(patch) },
        });
    }

    files.sort_unstable_by(|a, b| a.filename.cmp(&b.filename));
}

async fn merge_untracked_files(working_dir: Option<&str>, files: &mut Vec<ChangedFile>) {
    let untracked_output = match run_git_untracked(working_dir).await {
        Ok(output) => output,
        Err(_) => return,
    };
    let names = parse_path_list(&untracked_output);

    for filename in names {
        if files.iter().any(|f| f.filename == filename) {
            continue;
        }

        let patch = run_git_no_index_diff(working_dir, &filename)
            .await
            .unwrap_or_default();
        files.push(ChangedFile {
            filename: filename.clone(),
            status: "added".to_string(),
            additions: 0,
            deletions: 0,
            patch: if patch.is_empty() { None } else { Some(patch) },
        });
    }

    files.sort_unstable_by(|a, b| a.filename.cmp(&b.filename));
}

async fn fetch_and_send(repo: &str, pr_number: u32, tx: mpsc::Sender<DataLoadResult>) {
    match tokio::try_join!(
        github::fetch_pr(repo, pr_number),
        github::fetch_changed_files(repo, pr_number)
    ) {
        Ok((pr, mut files)) => {
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

async fn run_git_diff(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["diff", "HEAD"]).await
}

async fn run_git_numstat(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["diff", "--numstat", "HEAD"]).await
}

async fn run_git_name_only(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["diff", "--name-only", "HEAD"]).await
}

async fn current_head_sha(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["rev-parse", "HEAD"])
        .await
        .map(|s| s.trim().to_string())
}

async fn run_git_command(working_dir: Option<&str>, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
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
                path.to_string(),
                (parse_count(added_raw), parse_count(deleted_raw)),
            );
        }
    }

    result
}

fn parse_path_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn infer_status(additions: u32, deletions: u32) -> String {
    match (additions, deletions) {
        (0, _) => "removed".to_string(),
        (_, 0) => "added".to_string(),
        _ => "modified".to_string(),
    }
}

fn status_from_patch(patch: &str) -> Option<String> {
    if patch.contains("new file mode") {
        Some("added".to_string())
    } else if patch.contains("deleted file mode") {
        Some("removed".to_string())
    } else if patch.contains("rename from") || patch.contains("rename to") {
        Some("renamed".to_string())
    } else {
        Some("modified".to_string())
    }
}

fn build_changed_files(
    patches: &mut HashMap<String, String>,
    numstat: &HashMap<String, (u32, u32)>,
) -> Vec<ChangedFile> {
    let mut filenames: Vec<_> = patches.keys().cloned().collect();
    filenames.sort_unstable();

    let mut files = Vec::with_capacity(filenames.len());

    for filename in filenames {
        if let Some(patch) = patches.remove(&filename) {
            let (additions, deletions) = numstat.get(&filename).copied().unwrap_or((0, 0));
            let status = status_from_patch(&patch).unwrap_or_else(|| "modified".to_string());
            files.push(ChangedFile {
                filename,
                status,
                additions,
                deletions,
                patch: Some(patch),
            });
        }
    }

    files
}

async fn run_git_diff_file(working_dir: Option<&str>, filename: &str) -> Result<String> {
    run_git_command(working_dir, &["diff", "HEAD", "--", filename]).await
}

async fn run_git_untracked(working_dir: Option<&str>) -> Result<String> {
    run_git_command(working_dir, &["ls-files", "--others", "--exclude-standard"]).await
}

async fn run_git_no_index_diff(working_dir: Option<&str>, filename: &str) -> Result<String> {
    let mut command = Command::new("git");
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
    async fn test_fetch_local_diff_for_untracked_file_returns_full_patch() {
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

        let patch = new_file
            .patch
            .as_deref()
            .expect("untracked file should have a patch");
        assert!(patch.contains("new file mode"));
        assert!(patch.contains("+pub fn hello()"));
    }
}
