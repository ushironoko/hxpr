use anyhow::Result;
use clap::Parser;
use tokio::sync::mpsc;

mod app;
mod cache;
mod config;
mod editor;
mod github;
mod loader;
mod ui;

#[derive(Parser, Debug)]
#[command(name = "hxpr")]
#[command(about = "TUI for GitHub PR review, designed for Helix editor users")]
#[command(version)]
struct Args {
    /// Repository name (e.g., "owner/repo")
    #[arg(short, long)]
    repo: String,

    /// Pull request number
    #[arg(short, long)]
    pr: u32,

    /// Force refresh, ignore cache
    #[arg(long, default_value = "false")]
    refresh: bool,

    /// Cache TTL in seconds (default: 300 = 5 minutes)
    #[arg(long, default_value = "300")]
    cache_ttl: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = config::Config::load()?;

    // リトライ用のチャンネル
    let (retry_tx, mut retry_rx) = mpsc::channel::<()>(1);

    // キャッシュを同期的に読み込み（メインスレッドで即座に）
    let (mut app, tx, needs_fetch) = if args.refresh {
        let (app, tx) = app::App::new_loading(&args.repo, args.pr, config);
        (app, tx, loader::FetchMode::Fresh)
    } else {
        match cache::read_cache(&args.repo, args.pr, args.cache_ttl) {
            Ok(cache::CacheResult::Hit(entry)) => {
                let (app, tx) = app::App::new_with_cache(
                    &args.repo,
                    args.pr,
                    config,
                    entry.pr.clone(),
                    entry.files.clone(),
                );
                (app, tx, loader::FetchMode::CheckUpdate(entry.pr_updated_at))
            }
            Ok(cache::CacheResult::Stale(entry)) => {
                let (app, tx) = app::App::new_with_cache(
                    &args.repo,
                    args.pr,
                    config,
                    entry.pr.clone(),
                    entry.files.clone(),
                );
                (app, tx, loader::FetchMode::Fresh)
            }
            Ok(cache::CacheResult::Miss) | Err(_) => {
                let (app, tx) = app::App::new_loading(&args.repo, args.pr, config);
                (app, tx, loader::FetchMode::Fresh)
            }
        }
    };

    app.set_retry_sender(retry_tx);

    // バックグラウンドでAPI取得
    let repo = args.repo.clone();
    let pr_number = args.pr;

    tokio::spawn(async move {
        loader::fetch_pr_data(repo.clone(), pr_number, needs_fetch, tx.clone()).await;

        while retry_rx.recv().await.is_some() {
            let tx_retry = tx.clone();
            loader::fetch_pr_data(repo.clone(), pr_number, loader::FetchMode::Fresh, tx_retry)
                .await;
        }
    });

    app.run().await
}
