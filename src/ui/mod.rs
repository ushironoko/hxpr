mod ai_rally;
mod comment_list;
mod common;
mod diff_view;
mod file_list;
mod help;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use std::io::{self, Stdout};

use crate::app::{App, AppState, DataState};

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn render(frame: &mut Frame, app: &App) {
    // Loading状態の場合は専用画面を表示
    if matches!(app.data_state, DataState::Loading) {
        file_list::render_loading(frame, app);
        return;
    }
    if let DataState::Error(ref msg) = app.data_state {
        file_list::render_error(frame, app, msg);
        return;
    }

    match app.state {
        AppState::FileList => file_list::render(frame, app),
        AppState::DiffView => diff_view::render(frame, app),
        AppState::CommentPreview => diff_view::render_with_preview(frame, app),
        AppState::SuggestionPreview => diff_view::render_with_suggestion_preview(frame, app),
        AppState::CommentList => comment_list::render(frame, app),
        AppState::Help => help::render(frame, app),
        AppState::AiRally => ai_rally::render(frame, app),
    }
}
