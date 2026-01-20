use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::github::{self, ChangedFile, PullRequest};
use crate::loader::DataLoadResult;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    FileList,
    DiffView,
    CommentPreview,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReviewAction {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone)]
pub enum DataState {
    Loading,
    Loaded {
        pr: PullRequest,
        files: Vec<ChangedFile>,
    },
    Error(String),
}

pub struct App {
    pub repo: String,
    pub pr_number: u32,
    pub data_state: DataState,
    pub state: AppState,
    pub selected_file: usize,
    pub selected_line: usize,
    pub diff_line_count: usize,
    pub scroll_offset: usize,
    pub pending_comment: Option<String>,
    pub config: Config,
    pub should_quit: bool,
    data_receiver: Option<mpsc::Receiver<DataLoadResult>>,
    retry_sender: Option<mpsc::Sender<()>>,
}

impl App {
    /// Loading状態で開始（キャッシュミス時）
    pub fn new_loading(
        repo: &str,
        pr_number: u32,
        config: Config,
    ) -> (Self, mpsc::Sender<DataLoadResult>) {
        let (tx, rx) = mpsc::channel(2);

        let app = Self {
            repo: repo.to_string(),
            pr_number,
            data_state: DataState::Loading,
            state: AppState::FileList,
            selected_file: 0,
            selected_line: 0,
            diff_line_count: 0,
            scroll_offset: 0,
            pending_comment: None,
            config,
            should_quit: false,
            data_receiver: Some(rx),
            retry_sender: None,
        };

        (app, tx)
    }

    /// キャッシュデータで即座に開始（キャッシュヒット時）
    pub fn new_with_cache(
        repo: &str,
        pr_number: u32,
        config: Config,
        pr: PullRequest,
        files: Vec<ChangedFile>,
    ) -> (Self, mpsc::Sender<DataLoadResult>) {
        let (tx, rx) = mpsc::channel(2);

        let diff_line_count = Self::calc_diff_line_count(&files, 0);

        let app = Self {
            repo: repo.to_string(),
            pr_number,
            data_state: DataState::Loaded { pr, files },
            state: AppState::FileList,
            selected_file: 0,
            selected_line: 0,
            diff_line_count,
            scroll_offset: 0,
            pending_comment: None,
            config,
            should_quit: false,
            data_receiver: Some(rx),
            retry_sender: None,
        };

        (app, tx)
    }

    pub fn set_retry_sender(&mut self, tx: mpsc::Sender<()>) {
        self.retry_sender = Some(tx);
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = ui::setup_terminal()?;

        while !self.should_quit {
            self.poll_data_updates();
            terminal.draw(|frame| ui::render(frame, self))?;
            self.handle_input(&mut terminal).await?;
        }

        ui::restore_terminal(&mut terminal)?;
        Ok(())
    }

    /// バックグラウンドタスクからのデータ更新をポーリング
    fn poll_data_updates(&mut self) {
        let Some(ref mut rx) = self.data_receiver else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => self.handle_data_result(result),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.data_receiver = None;
            }
        }
    }

    fn handle_data_result(&mut self, result: DataLoadResult) {
        match result {
            DataLoadResult::Success { pr, files } => {
                self.diff_line_count = Self::calc_diff_line_count(&files, self.selected_file);
                self.data_state = DataState::Loaded { pr, files };
            }
            DataLoadResult::Error(msg) => {
                // Loading状態の場合のみエラー表示（既にデータがある場合は無視）
                if matches!(self.data_state, DataState::Loading) {
                    self.data_state = DataState::Error(msg);
                }
            }
        }
    }

    fn calc_diff_line_count(files: &[ChangedFile], selected: usize) -> usize {
        files
            .get(selected)
            .and_then(|f| f.patch.as_ref())
            .map(|p| p.lines().count())
            .unwrap_or(0)
    }

    pub fn files(&self) -> &[ChangedFile] {
        match &self.data_state {
            DataState::Loaded { files, .. } => files,
            _ => &[],
        }
    }

    pub fn pr(&self) -> Option<&PullRequest> {
        match &self.data_state {
            DataState::Loaded { pr, .. } => Some(pr),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn is_data_available(&self) -> bool {
        matches!(self.data_state, DataState::Loaded { .. })
    }

    async fn handle_input(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Error状態でのリトライ処理
                if let DataState::Error(_) = &self.data_state {
                    match key.code {
                        KeyCode::Char('q') => self.should_quit = true,
                        KeyCode::Char('r') => self.retry_load(),
                        _ => {}
                    }
                    return Ok(());
                }

                // Loading状態ではqのみ受け付け
                if matches!(self.data_state, DataState::Loading) {
                    if key.code == KeyCode::Char('q') {
                        self.should_quit = true;
                    }
                    return Ok(());
                }

                match self.state {
                    AppState::FileList => self.handle_file_list_input(key, terminal).await?,
                    AppState::DiffView => self.handle_diff_view_input(key, terminal).await?,
                    AppState::CommentPreview => self.handle_comment_preview_input(key).await?,
                    AppState::Help => self.handle_help_input(key)?,
                }
            }
        }
        Ok(())
    }

    fn retry_load(&mut self) {
        if let Some(ref tx) = self.retry_sender {
            self.data_state = DataState::Loading;
            let _ = tx.try_send(());
        }
    }

    async fn handle_file_list_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.files().is_empty() {
                    self.selected_file =
                        (self.selected_file + 1).min(self.files().len().saturating_sub(1));
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_file = self.selected_file.saturating_sub(1);
            }
            KeyCode::Enter => {
                if !self.files().is_empty() {
                    self.state = AppState::DiffView;
                    self.selected_line = 0;
                    self.scroll_offset = 0;
                    self.update_diff_line_count();
                }
            }
            KeyCode::Char(c) if c == self.config.keybindings.approve => {
                self.submit_review(ReviewAction::Approve, terminal).await?
            }
            KeyCode::Char(c) if c == self.config.keybindings.request_changes => {
                self.submit_review(ReviewAction::RequestChanges, terminal)
                    .await?
            }
            KeyCode::Char(c) if c == self.config.keybindings.comment => {
                self.submit_review(ReviewAction::Comment, terminal).await?
            }
            KeyCode::Char('?') => self.state = AppState::Help,
            _ => {}
        }
        Ok(())
    }

    async fn handle_diff_view_input(
        &mut self,
        key: event::KeyEvent,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let visible_lines = terminal.size()?.height.saturating_sub(8) as usize;

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.state = AppState::FileList,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.diff_line_count > 0 {
                    self.selected_line =
                        (self.selected_line + 1).min(self.diff_line_count.saturating_sub(1));
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_line = self.selected_line.saturating_sub(1);
                self.adjust_scroll(visible_lines);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.diff_line_count > 0 {
                    self.selected_line =
                        (self.selected_line + 20).min(self.diff_line_count.saturating_sub(1));
                    self.adjust_scroll(visible_lines);
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.selected_line = self.selected_line.saturating_sub(20);
                self.adjust_scroll(visible_lines);
            }
            KeyCode::Char(c) if c == self.config.keybindings.comment => {
                self.open_comment_editor(terminal).await?
            }
            _ => {}
        }
        Ok(())
    }

    fn adjust_scroll(&mut self, visible_lines: usize) {
        if visible_lines == 0 {
            return;
        }
        if self.selected_line < self.scroll_offset {
            self.scroll_offset = self.selected_line;
        }
        if self.selected_line >= self.scroll_offset + visible_lines {
            self.scroll_offset = self.selected_line.saturating_sub(visible_lines) + 1;
        }
    }

    async fn handle_comment_preview_input(&mut self, key: event::KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter => {
                if let Some(body) = self.pending_comment.take() {
                    if let Some(file) = self.files().get(self.selected_file) {
                        if let Some(pr) = self.pr() {
                            let commit_id = pr.head.sha.clone();
                            let filename = file.filename.clone();
                            github::create_review_comment(
                                &self.repo,
                                self.pr_number,
                                &commit_id,
                                &filename,
                                self.selected_line as u32,
                                &body,
                            )
                            .await?;
                        }
                    }
                }
                self.state = AppState::DiffView;
            }
            KeyCode::Esc => {
                self.pending_comment = None;
                self.state = AppState::DiffView;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_help_input(&mut self, key: event::KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                self.state = AppState::FileList;
            }
            _ => {}
        }
        Ok(())
    }

    async fn open_comment_editor(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        let Some(file) = self.files().get(self.selected_file) else {
            return Ok(());
        };
        let filename = file.filename.clone();

        ui::restore_terminal(terminal)?;

        let comment =
            crate::editor::open_comment_editor(&self.config.editor, &filename, self.selected_line)?;

        *terminal = ui::setup_terminal()?;

        if let Some(body) = comment {
            self.pending_comment = Some(body);
            self.state = AppState::CommentPreview;
        }
        Ok(())
    }

    async fn submit_review(
        &mut self,
        action: ReviewAction,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        ui::restore_terminal(terminal)?;

        let body = crate::editor::open_review_editor(&self.config.editor)?;

        *terminal = ui::setup_terminal()?;

        if let Some(body) = body {
            github::submit_review(&self.repo, self.pr_number, action, &body).await?;
        }
        Ok(())
    }

    fn update_diff_line_count(&mut self) {
        self.diff_line_count = Self::calc_diff_line_count(self.files(), self.selected_file);
    }
}
