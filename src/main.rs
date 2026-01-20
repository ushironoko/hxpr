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

    // TUI即時表示用のApp作成
    let (mut app, tx) = app::App::new_loading(&args.repo, args.pr, config);

    // リトライ用のチャンネル
    let (retry_tx, mut retry_rx) = mpsc::channel::<()>(1);
    app.set_retry_sender(retry_tx);

    // バックグラウンドでデータロード開始
    let repo = args.repo.clone();
    let pr_number = args.pr;
    let refresh = args.refresh;
    let cache_ttl = args.cache_ttl;

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        loader::load_pr_data(repo.clone(), pr_number, refresh, cache_ttl, tx_clone).await;

        // リトライ要求を待機
        while retry_rx.recv().await.is_some() {
            let tx_retry = tx.clone();
            loader::load_pr_data(repo.clone(), pr_number, true, cache_ttl, tx_retry).await;
        }
    });

    // TUI実行
    app.run().await
}
