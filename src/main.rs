use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use std::io;
use std::panic;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

mod ai;
mod app;
mod cache;
mod config;
mod diff;
mod editor;
mod github;
mod init;
mod loader;
mod syntax;
mod ui;

#[derive(Parser, Debug)]
#[command(name = "or")]
#[command(about = "TUI for GitHub PR review, designed for Helix editor users")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Repository name (e.g., "owner/repo")
    #[arg(short, long)]
    repo: Option<String>,

    /// Pull request number
    #[arg(short, long)]
    pr: Option<u32>,

    /// Force refresh, ignore cache
    #[arg(long, default_value = "false")]
    refresh: bool,

    /// Cache TTL in seconds (default: 300 = 5 minutes)
    #[arg(long, default_value = "300")]
    cache_ttl: u64,

    /// Start AI Rally mode directly
    #[arg(long, default_value = "false")]
    ai_rally: bool,

    /// Working directory for AI agents (default: current directory)
    #[arg(long)]
    working_dir: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize configuration files and prompt templates
    Init {
        /// Force overwrite existing files
        #[arg(long, default_value = "false")]
        force: bool,
    },
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

    let args = Args::parse();

    // Handle subcommands
    if let Some(command) = args.command {
        return match command {
            Commands::Init { force } => init::run_init(force),
        };
    }

    // For main PR review mode, repo and pr are required
    let repo = args.repo.ok_or_else(|| {
        anyhow::anyhow!("--repo is required. Use 'or --repo owner/repo --pr 123'")
    })?;
    let pr = args
        .pr
        .ok_or_else(|| anyhow::anyhow!("--pr is required. Use 'or --repo owner/repo --pr 123'"))?;

    // Pre-initialize syntax highlighting in background to avoid delay on first diff view
    std::thread::spawn(|| {
        let _ = syntax::syntax_set();
        let _ = syntax::theme_set();
    });

    let config = config::Config::load()?;

    // リトライ用のチャンネル
    let (retry_tx, mut retry_rx) = mpsc::channel::<()>(1);

    // キャッシュを同期的に読み込み（メインスレッドで即座に）
    let (mut app, tx, needs_fetch) = if args.refresh {
        // --refresh 時は全キャッシュを削除
        let _ = cache::invalidate_all_cache(&repo, pr);
        let (app, tx) = app::App::new_loading(&repo, pr, config);
        (app, tx, loader::FetchMode::Fresh)
    } else {
        match cache::read_cache(&repo, pr, args.cache_ttl) {
            Ok(cache::CacheResult::Hit(entry)) => {
                let pr_updated_at = entry.pr_updated_at;
                let (app, tx) = app::App::new_with_cache(&repo, pr, config, entry.pr, entry.files);
                (app, tx, loader::FetchMode::CheckUpdate(pr_updated_at))
            }
            Ok(cache::CacheResult::Stale(entry)) => {
                let (app, tx) = app::App::new_with_cache(&repo, pr, config, entry.pr, entry.files);
                (app, tx, loader::FetchMode::Fresh)
            }
            Ok(cache::CacheResult::Miss) | Err(_) => {
                let (app, tx) = app::App::new_loading(&repo, pr, config);
                (app, tx, loader::FetchMode::Fresh)
            }
        }
    };

    app.set_retry_sender(retry_tx);

    // Set working directory for AI agents
    if let Some(dir) = args.working_dir.clone() {
        app.set_working_dir(Some(dir));
    } else {
        // Use current directory as default.
        // Note: current_dir() can fail in edge cases (e.g., if the current directory
        // has been deleted, or on some restricted environments). When --ai-rally is
        // used without --working-dir, we need a valid directory for the AI agents.
        match std::env::current_dir() {
            Ok(cwd) => {
                app.set_working_dir(Some(cwd.to_string_lossy().to_string()));
            }
            Err(e) => {
                if args.ai_rally {
                    eprintln!(
                        "Warning: Failed to get current directory: {}. AI Rally may not work correctly without --working-dir.",
                        e
                    );
                }
                // Continue without setting working_dir; it's optional for non-AI-Rally usage
            }
        }
    }

    // Set flag to start AI Rally mode when --ai-rally is passed
    if args.ai_rally {
        app.set_start_ai_rally_on_load(true);
    }

    // Cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    // バックグラウンドでAPI取得
    let repo_clone = repo.clone();
    let pr_number = pr;

    tokio::spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {}
            _ = async {
                loader::fetch_pr_data(repo_clone.clone(), pr_number, needs_fetch, tx.clone()).await;

                while retry_rx.recv().await.is_some() {
                    let tx_retry = tx.clone();
                    loader::fetch_pr_data(repo_clone.clone(), pr_number, loader::FetchMode::Fresh, tx_retry)
                        .await;
                }
            } => {}
        }
    });

    // Run the app and ensure terminal is restored on error
    let result = app.run().await;

    // Signal background tasks to stop
    cancel_token.cancel();

    // 終了時にキャッシュを削除
    let _ = cache::invalidate_all_cache(&repo, pr);

    if result.is_err() {
        restore_terminal();
    }
    result
}
