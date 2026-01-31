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

// Use modules from the library crate
use octorus::{app, cache, config, github, loader, syntax};

// init is only used by the binary, not needed for benchmarks
mod init;

#[derive(Parser, Debug)]
#[command(name = "or")]
#[command(about = "TUI for GitHub PR review, designed for Helix editor users")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Repository name (e.g., "owner/repo"). Auto-detected from current directory if omitted.
    #[arg(short, long)]
    repo: Option<String>,

    /// Pull request number. Shows PR list if omitted.
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

    // Detect or use provided repo
    let repo = match args.repo.clone() {
        Some(r) => r,
        None => match github::detect_repo().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
    };

    // Pre-initialize syntax highlighting in background to avoid delay on first diff view
    std::thread::spawn(|| {
        let _ = syntax::syntax_set();
        let _ = syntax::theme_set();
    });

    let config = config::Config::load()?;

    // Check if we have a specific PR number
    if let Some(pr) = args.pr {
        // Existing flow: open specific PR
        run_with_pr(&repo, pr, &config, &args).await
    } else {
        // New flow: show PR list
        run_with_pr_list(&repo, config, &args).await
    }
}

/// Run the app with a specific PR number (existing flow)
async fn run_with_pr(repo: &str, pr: u32, config: &config::Config, args: &Args) -> Result<()> {
    // リトライ用のチャンネル
    let (retry_tx, mut retry_rx) = mpsc::channel::<()>(1);

    // キャッシュを同期的に読み込み（メインスレッドで即座に）
    let (mut app, tx, needs_fetch) = if args.refresh {
        // --refresh 時は全キャッシュを削除
        let _ = cache::invalidate_all_cache(repo, pr);
        let (app, tx) = app::App::new_loading(repo, pr, config.clone());
        (app, tx, loader::FetchMode::Fresh)
    } else {
        match cache::read_cache(repo, pr, args.cache_ttl) {
            Ok(cache::CacheResult::Hit(entry)) => {
                let pr_updated_at = entry.pr_updated_at;
                let (app, tx) =
                    app::App::new_with_cache(repo, pr, config.clone(), entry.pr, entry.files);
                (app, tx, loader::FetchMode::CheckUpdate(pr_updated_at))
            }
            Ok(cache::CacheResult::Stale(entry)) => {
                let (app, tx) =
                    app::App::new_with_cache(repo, pr, config.clone(), entry.pr, entry.files);
                (app, tx, loader::FetchMode::Fresh)
            }
            Ok(cache::CacheResult::Miss) | Err(_) => {
                let (app, tx) = app::App::new_loading(repo, pr, config.clone());
                (app, tx, loader::FetchMode::Fresh)
            }
        }
    };

    app.set_retry_sender(retry_tx);
    setup_working_dir(&mut app, args);

    // Set flag to start AI Rally mode when --ai-rally is passed
    if args.ai_rally {
        app.set_start_ai_rally_on_load(true);
    }

    // Cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    // バックグラウンドでAPI取得
    let repo_clone = repo.to_string();
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
    let _ = cache::invalidate_all_cache(repo, pr);

    if result.is_err() {
        restore_terminal();
    }
    result
}

/// Run the app with PR list (new flow)
async fn run_with_pr_list(repo: &str, config: config::Config, args: &Args) -> Result<()> {
    let mut app = app::App::new_pr_list(repo, config);
    setup_working_dir(&mut app, args);

    // Set pending AI Rally flag if --ai-rally was passed
    if args.ai_rally {
        app.set_pending_ai_rally(true);
    }

    // Start loading PR list
    let (tx, rx) = mpsc::channel(2);
    app.set_pr_list_receiver(rx);

    let repo_clone = repo.to_string();
    let state_filter = app.pr_list_state_filter;

    tokio::spawn(async move {
        let result = github::fetch_pr_list(&repo_clone, state_filter, 30).await;
        let _ = tx.send(result.map_err(|e| e.to_string())).await;
    });

    // Run the app
    let result = app.run().await;

    if result.is_err() {
        restore_terminal();
    }
    result
}

/// Set up working directory for AI agents
fn setup_working_dir(app: &mut app::App, args: &Args) {
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
}
