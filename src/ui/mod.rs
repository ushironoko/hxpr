mod ai_rally;
mod comment_list;
mod common;
pub mod diff_view;
mod file_list;
mod help;
mod split_view;
pub mod text_area;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem},
    Frame, Terminal,
};
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

pub fn render(frame: &mut Frame, app: &mut App) {
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
        AppState::TextInput => diff_view::render_text_input(frame, app),
        AppState::CommentList => comment_list::render(frame, app),
        AppState::Help => help::render(frame, app),
        AppState::AiRally => ai_rally::render(frame, app),
        AppState::SplitViewFileList | AppState::SplitViewDiff => split_view::render(frame, app),
    }

    // シンボル選択ポップアップ（最前面に描画）
    if let Some(ref popup) = app.symbol_popup {
        render_symbol_popup(frame, popup);
    }
}

/// 中央配置のフローティングポップアップ領域を計算
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// シンボル選択ポップアップを描画
fn render_symbol_popup(frame: &mut Frame, popup: &crate::app::SymbolPopupState) {
    let area = frame.area();

    // ポップアップサイズ計算
    let max_width = popup
        .symbols
        .iter()
        .map(|(name, _, _)| name.len())
        .max()
        .unwrap_or(10) as u16
        + 6; // padding + borders
    let height = (popup.symbols.len() as u16 + 2).min(area.height.saturating_sub(4)); // +2 for borders
    let width = max_width.max(20).min(area.width.saturating_sub(4));

    let popup_area = centered_rect(width, height, area);

    // 背景クリア
    frame.render_widget(Clear, popup_area);

    // リストアイテム作成
    let items: Vec<ListItem> = popup
        .symbols
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| {
            let style = if i == popup.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(format!("  {}  ", name), style)))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Select symbol (j/k/↑↓: move, Enter: jump, Esc: cancel)")
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, popup_area);
}
