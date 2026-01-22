use anyhow::Result;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use std::io;
use std::panic;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod app;
mod cache;
mod config;
mod diff;
mod editor;
mod github;
mod loader;
mod syntax;
mod ui;

#[derive(Parser, Debug)]
#[command(name = "or")]
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

/// Restore terminal to normal state
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

/// Set up panic hook to restore terminal on panic
fn setup_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore_terminal();
        original_hook(panic_info);
    }));
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up panic hook before anything else
    setup_panic_hook();

    // Pre-initialize syntax highlighting in background to avoid delay on first diff view
    std::thread::spawn(|| {
        let _ = syntax::syntax_set();
        let _ = syntax::theme_set();
    });

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
                let pr_updated_at = entry.pr_updated_at;
                let (app, tx) =
                    app::App::new_with_cache(&args.repo, args.pr, config, entry.pr, entry.files);
                (app, tx, loader::FetchMode::CheckUpdate(pr_updated_at))
            }
            Ok(cache::CacheResult::Stale(entry)) => {
                let (app, tx) =
                    app::App::new_with_cache(&args.repo, args.pr, config, entry.pr, entry.files);
                (app, tx, loader::FetchMode::Fresh)
            }
            Ok(cache::CacheResult::Miss) | Err(_) => {
                let (app, tx) = app::App::new_loading(&args.repo, args.pr, config);
                (app, tx, loader::FetchMode::Fresh)
            }
        }
    };

    app.set_retry_sender(retry_tx);

    // Cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    // バックグラウンドでAPI取得
    let repo = args.repo.clone();
    let pr_number = args.pr;

    tokio::spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {}
            _ = async {
                loader::fetch_pr_data(repo.clone(), pr_number, needs_fetch, tx.clone()).await;

                while retry_rx.recv().await.is_some() {
                    let tx_retry = tx.clone();
                    loader::fetch_pr_data(repo.clone(), pr_number, loader::FetchMode::Fresh, tx_retry)
                        .await;
                }
            } => {}
        }
    });

    // Run the app and ensure terminal is restored on error
    let result = app.run().await;

    // Signal background tasks to stop
    cancel_token.cancel();

    if result.is_err() {
        restore_terminal();
    }
    result
}
