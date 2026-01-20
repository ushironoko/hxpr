use tokio::sync::mpsc;

use crate::cache::{self, CacheResult};
use crate::github::{self, ChangedFile, PullRequest};

pub enum DataLoadResult {
    /// APIまたはキャッシュからデータ取得成功
    Success {
        pr: PullRequest,
        files: Vec<ChangedFile>,
    },
    /// キャッシュヒット（バックグラウンド更新中）
    CacheHit {
        pr: PullRequest,
        files: Vec<ChangedFile>,
        checking_update: bool,
    },
    /// エラー
    Error(String),
}

pub async fn load_pr_data(
    repo: String,
    pr_number: u32,
    refresh: bool,
    cache_ttl: u64,
    tx: mpsc::Sender<DataLoadResult>,
) {
    // --refresh の場合はキャッシュをスキップ
    if !refresh {
        match cache::read_cache(&repo, pr_number, cache_ttl) {
            Ok(CacheResult::Hit(entry)) => {
                // 有効なキャッシュ → 即座に返す
                let _ = tx
                    .send(DataLoadResult::CacheHit {
                        pr: entry.pr.clone(),
                        files: entry.files.clone(),
                        checking_update: true,
                    })
                    .await;

                // バックグラウンドで更新チェック
                check_for_updates(&repo, pr_number, &entry.pr_updated_at, tx).await;
                return;
            }
            Ok(CacheResult::Stale(entry)) => {
                // TTL切れキャッシュ → 一旦表示して再取得
                let _ = tx
                    .send(DataLoadResult::CacheHit {
                        pr: entry.pr.clone(),
                        files: entry.files.clone(),
                        checking_update: true,
                    })
                    .await;

                fetch_and_send(&repo, pr_number, tx).await;
                return;
            }
            Ok(CacheResult::Miss) | Err(_) => {
                // キャッシュなし
            }
        }
    }

    fetch_and_send(&repo, pr_number, tx).await;
}

async fn fetch_and_send(repo: &str, pr_number: u32, tx: mpsc::Sender<DataLoadResult>) {
    match tokio::try_join!(
        github::fetch_pr(repo, pr_number),
        github::fetch_changed_files(repo, pr_number)
    ) {
        Ok((pr, files)) => {
            let _ = cache::write_cache(repo, pr_number, &pr, &files);
            let _ = tx.send(DataLoadResult::Success { pr, files }).await;
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
        // 更新なし → 何もしない（既にキャッシュデータを送信済み）
    }
}
