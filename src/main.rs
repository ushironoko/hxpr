use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::io;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// Use modules from the library crate
use octorus::app::RefreshRequest;
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

    /// Start AI Rally mode directly
    #[arg(long, default_value = "false")]
    ai_rally: bool,

    /// Show local git diff against current HEAD (no GitHub PR fetch)
    #[arg(long, default_value = "false")]
    local: bool,

    /// Auto-focus changed file when local diff updates (for local mode)
    #[arg(long, default_value = "false")]
    auto_focus: bool,

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
    /// Remove AI Rally session data
    Clean,
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

    // OR_DEBUG=1 でファイルログを有効化（TUI の画面を壊さないよう stderr ではなくファイルに出力）
    if std::env::var("OR_DEBUG").ok().as_deref() == Some("1") {
        let log_dir = cache::cache_dir();
        if std::fs::create_dir_all(&log_dir).is_ok() {
            if let Ok(log_file) = std::fs::File::options()
                .create(true)
                .append(true)
                .open(log_dir.join("debug.log"))
            {
                use tracing_subscriber::EnvFilter;
                tracing_subscriber::fmt()
                    .with_writer(std::sync::Mutex::new(log_file))
                    .with_env_filter(EnvFilter::new("octorus=debug,or=debug"))
                    .init();
                tracing::info!("Debug logging enabled");
            }
        }
    }

    let args = Args::parse();

    // Handle subcommands
    if let Some(command) = args.command {
        return match command {
            Commands::Init { force } => init::run_init(force),
            Commands::Clean => {
                cache::cleanup_rally_sessions();
                let rally_dir = cache::cache_dir().join("rally");
                println!("Rally sessions cleaned: {}", rally_dir.display());
                Ok(())
            }
        };
    }

    let repo = if args.local {
        args.repo.clone().unwrap_or_else(|| "local".to_string())
    } else {
        // Detect or use provided repo
        match args.repo.clone() {
            Some(r) => r,
            None => match github::detect_repo().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            },
        }
    };

    // Pre-initialize syntax highlighting in background to avoid delay on first diff view
    std::thread::spawn(|| {
        let _ = syntax::syntax_set();
        let _ = syntax::theme_set();
    });

    let config = config::Config::load()?;

    if args.local {
        run_with_local_diff(&repo, &config, &args).await
    } else if let Some(pr) = args.pr {
        run_with_pr(&repo, pr, &config, &args).await
    } else {
        run_with_pr_list(&repo, config, &args).await
    }
}

async fn run_with_local_diff(repo: &str, config: &config::Config, args: &Args) -> Result<()> {
    let (retry_tx, mut retry_rx) = mpsc::channel::<RefreshRequest>(1);
    let (mut app, tx) = app::App::new_loading(repo, 0, config.clone());
    let working_dir = args.working_dir.clone();
    let refresh_pending = Arc::new(AtomicBool::new(false));

    app.set_retry_sender(retry_tx.clone());
    setup_local_watch(retry_tx, working_dir.clone(), refresh_pending.clone());
    app.set_local_mode(true);
    app.set_local_auto_focus(args.auto_focus);
    setup_working_dir(&mut app, args);

    if args.ai_rally {
        app.set_start_ai_rally_on_load(true);
    }

    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();
    let repo = repo.to_string();

    loader::fetch_local_diff(repo.clone(), working_dir.clone(), tx.clone()).await;

    tokio::spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {}
            _ = async {
                while let Some(request) = retry_rx.recv().await {
                    match request {
                        RefreshRequest::LocalRefresh => {
                            refresh_pending.store(false, Ordering::Release);

                            loop {
                                let tx_retry = tx.clone();
                                loader::fetch_local_diff(repo.clone(), working_dir.clone(), tx_retry).await;

                                if !refresh_pending.swap(false, Ordering::AcqRel) {
                                    break;
                                }
                            }
                        }
                        RefreshRequest::PrRefresh { .. } => {
                            // ローカルモードでは PrRefresh を無視する。
                            // pr_number == 0 の擬似値で API 呼び出しすると無効なリクエストになるため、
                            // LocalRefresh として処理する。
                            let tx_retry = tx.clone();
                            loader::fetch_local_diff(repo.clone(), working_dir.clone(), tx_retry).await;
                        }
                    }
                }
            } => {}
        }
    });

    let result = app.run().await;
    cancel_token.cancel();

    if let Err(ref e) = result {
        restore_terminal();
        eprintln!("Error: {:#}", e);
    }

    let exit_code = if result.is_ok() { 0 } else { 1 };
    std::process::exit(exit_code);
}

fn setup_local_watch(
    refresh_tx: mpsc::Sender<RefreshRequest>,
    working_dir: Option<String>,
    refresh_pending: Arc<AtomicBool>,
) {
    let watch_dir = working_dir.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    });

    std::thread::spawn({
        let refresh_tx = refresh_tx.clone();
        move || {
            let callback = move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else {
                    return;
                };

                let should_refresh = should_refresh_local_change(&event.paths, &event.kind);

                if should_refresh && !refresh_pending.swap(true, Ordering::AcqRel) {
                    let _ = refresh_tx.try_send(RefreshRequest::LocalRefresh);
                }
            };

            let Ok(mut watcher) = RecommendedWatcher::new(callback, Config::default()) else {
                return;
            };

            let _ = watcher.watch(Path::new(&watch_dir), RecursiveMode::Recursive);

            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
    });
}

fn should_refresh_local_change(paths: &[PathBuf], kind: &EventKind) -> bool {
    !matches!(kind, EventKind::Access(_)) && paths.iter().any(|path| !is_git_file(path))
}

fn is_git_file(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == ".git")
}

/// Run the app with a specific PR number (existing flow)
async fn run_with_pr(repo: &str, pr: u32, config: &config::Config, args: &Args) -> Result<()> {
    // リトライ用のチャンネル
    let (retry_tx, mut retry_rx) = mpsc::channel::<RefreshRequest>(1);
    let refresh_pending = Arc::new(AtomicBool::new(false));

    // 常に Loading 状態で開始し、バックグラウンドで API 取得
    let (mut app, tx) = app::App::new_loading(repo, pr, config.clone());

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
    let working_dir = args.working_dir.clone();

    tokio::spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {}
            _ = async {
                loader::fetch_pr_data(repo_clone.clone(), pr_number, loader::FetchMode::Fresh, tx.clone()).await;

                while let Some(request) = retry_rx.recv().await {
                    match request {
                        RefreshRequest::PrRefresh { pr_number } => {
                            let tx_retry = tx.clone();
                            loader::fetch_pr_data(repo_clone.clone(), pr_number, loader::FetchMode::Fresh, tx_retry)
                                .await;
                        }
                        RefreshRequest::LocalRefresh => {
                            refresh_pending.store(false, Ordering::Release);
                            loop {
                                let tx_retry = tx.clone();
                                loader::fetch_local_diff(repo_clone.clone(), working_dir.clone(), tx_retry).await;
                                if !refresh_pending.swap(false, Ordering::AcqRel) {
                                    break;
                                }
                            }
                        }
                    }
                }
            } => {}
        }
    });

    // Run the app and ensure terminal is restored on error
    let result = app.run().await;

    // Signal background tasks to stop
    cancel_token.cancel();

    if let Err(ref e) = result {
        restore_terminal();
        eprintln!("Error: {:#}", e);
    }

    // spawn_blocking タスク（プリフェッチ等）が巨大ファイル処理中の場合、
    // tokio ランタイムの drop が完了を待ち続けるため、即座にプロセスを終了する。
    // これにより Drop ベースのクリーンアップはスキップされるが、バックグラウンドタスクは
    // cancel_token.cancel() で明示的に停止済みであり、残るのは spawn_blocking の
    // tree-sitter パース処理のみ。OS がプロセス終了時にリソースを回収するため問題なし。
    let exit_code = if result.is_ok() { 0 } else { 1 };
    std::process::exit(exit_code);
}

/// Run the app with PR list (new flow)
async fn run_with_pr_list(repo: &str, config: config::Config, args: &Args) -> Result<()> {
    // リトライ用のチャンネル（PR リスト画面から Local モードへの切替に対応）
    let (retry_tx, mut retry_rx) = mpsc::channel::<RefreshRequest>(1);
    let refresh_pending = Arc::new(AtomicBool::new(false));

    let mut app = app::App::new_pr_list(repo, config);
    app.set_retry_sender(retry_tx);
    setup_working_dir(&mut app, args);

    // Set pending AI Rally flag if --ai-rally was passed
    if args.ai_rally {
        app.set_pending_ai_rally(true);
    }

    // Start loading PR list
    let (pr_list_tx, rx) = mpsc::channel(2);
    app.set_pr_list_receiver(rx);

    let repo_clone = repo.to_string();
    let state_filter = app.pr_list_state_filter;

    tokio::spawn(async move {
        let result = github::fetch_pr_list(&repo_clone, state_filter, 30).await;
        let _ = pr_list_tx.send(result.map_err(|e| e.to_string())).await;
    });

    // データ取得用チャンネル（Local モード切替時に使用）
    let (data_tx, data_rx) = mpsc::channel(2);
    app.set_data_receiver(0, data_rx);

    // Cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    // リトライループ（Local/PR リフレッシュ対応）
    let repo_for_retry = repo.to_string();
    let working_dir = args.working_dir.clone();

    tokio::spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {}
            _ = async {
                while let Some(request) = retry_rx.recv().await {
                    match request {
                        RefreshRequest::PrRefresh { pr_number } => {
                            let tx_retry = data_tx.clone();
                            loader::fetch_pr_data(repo_for_retry.clone(), pr_number, loader::FetchMode::Fresh, tx_retry)
                                .await;
                        }
                        RefreshRequest::LocalRefresh => {
                            refresh_pending.store(false, Ordering::Release);
                            loop {
                                let tx_retry = data_tx.clone();
                                loader::fetch_local_diff(repo_for_retry.clone(), working_dir.clone(), tx_retry).await;
                                if !refresh_pending.swap(false, Ordering::AcqRel) {
                                    break;
                                }
                            }
                        }
                    }
                }
            } => {}
        }
    });

    // Run the app
    let result = app.run().await;

    // Signal background tasks to stop
    cancel_token.cancel();

    if let Err(ref e) = result {
        restore_terminal();
        eprintln!("Error: {:#}", e);
    }

    // run_with_pr と同様、spawn_blocking タスクの完了待ちによるハングを防止するため
    // 即座にプロセスを終了する。バックグラウンドタスクやサブプロセスの明示的な停止は
    // app.run() 内で完了済み。
    let exit_code = if result.is_ok() { 0 } else { 1 };
    std::process::exit(exit_code);
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

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, AccessMode, CreateKind};
    use std::path::PathBuf;

    #[test]
    fn test_should_refresh_local_change_ignores_access_events() {
        let paths = vec![PathBuf::from("src/main.rs")];
        let kind = EventKind::Access(AccessKind::Close(AccessMode::Write));

        assert!(!should_refresh_local_change(&paths, &kind));
    }

    #[test]
    fn test_should_refresh_local_change_ignores_git_paths() {
        let paths = vec![PathBuf::from(".git/HEAD"), PathBuf::from(".git/index.lock")];
        let kind = EventKind::Create(CreateKind::File);

        assert!(!should_refresh_local_change(&paths, &kind));
    }

    #[test]
    fn test_should_refresh_local_change_refreshes_subdir_change() {
        let paths = vec![
            PathBuf::from(".git/HEAD"),
            PathBuf::from("src/subdir/changed.rs"),
        ];
        let kind = EventKind::Create(CreateKind::File);

        assert!(should_refresh_local_change(&paths, &kind));
    }

    #[test]
    fn test_is_git_file_identifies_git_path() {
        assert!(is_git_file(std::path::Path::new(".git/refs/heads/main")));
        assert!(!is_git_file(std::path::Path::new("src/main.rs")));
    }
}
