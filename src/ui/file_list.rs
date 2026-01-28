use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::common::render_rally_status_bar;
use crate::app::{App, DataState};
use crate::github::ChangedFile;

pub fn render(frame: &mut Frame, app: &App) {
    let has_rally = app.has_background_rally();
    let constraints = if has_rally {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(0),    // File list
            Constraint::Length(1), // Rally status bar
            Constraint::Length(3), // Footer
        ]
    } else {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(0),    // File list
            Constraint::Length(3), // Footer
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    // Header
    let pr_info = match &app.data_state {
        DataState::Loaded { pr, .. } => {
            format!("PR #{}: {} by @{}", pr.number, pr.title, pr.user.login)
        }
        _ => format!("PR #{}", app.pr_number),
    };

    let header =
        Paragraph::new(pr_info).block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // File list
    let files = app.files();
    let items = build_file_list_items(files, app.selected_file);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Changed Files ({})", files.len())),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_widget(list, chunks[1]);

    // Rally status bar (if background rally exists)
    if has_rally {
        render_rally_status_bar(frame, chunks[2], app);
    }

    // Footer (dynamic based on rally state)
    let footer_chunk_idx = if has_rally { 3 } else { 2 };
    let ai_rally_text = if app.has_background_rally() {
        "A: Resume Rally"
    } else {
        "A: AI Rally"
    };
    let footer_text = format!(
        "j/k: move | Enter/→/l: split view | a: approve | r: request changes | c: comment | C: comments | {} | R: refresh | q: quit | ?: help",
        ai_rally_text
    );
    let footer = Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[footer_chunk_idx]);
}

/// Loading状態の表示
pub fn render_loading(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());

    // Header
    let header = Paragraph::new(format!("PR #{} - Loading...", app.pr_number))
        .block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // Loading message
    let loading = Paragraph::new(format!("{} Loading PR data...", app.spinner_char()))
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Changed Files"),
        );
    frame.render_widget(loading, chunks[1]);

    // Footer
    let footer = Paragraph::new(format!("{} Please wait... (q: quit)", app.spinner_char()))
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

/// Error状態の表示
pub fn render_error(frame: &mut Frame, app: &App, error_msg: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(frame.area());

    // Header
    let header = Paragraph::new(format!("PR #{} - Error", app.pr_number))
        .block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // Error message
    let error = Paragraph::new(format!("Error: {}", error_msg))
        .style(Style::default().fg(Color::Red))
        .block(Block::default().borders(Borders::ALL).title("Error"));
    frame.render_widget(error, chunks[1]);

    // Footer
    let footer = Paragraph::new("r: retry | q: quit").block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

/// ファイル一覧のリストアイテムを構築する（side_by_side でも再利用）
pub(crate) fn build_file_list_items<'a>(
    files: &'a [ChangedFile],
    selected_file: usize,
) -> Vec<ListItem<'a>> {
    files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let style = if i == selected_file {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let status_color = match file.status.as_str() {
                "added" => Color::Green,
                "removed" => Color::Red,
                "modified" => Color::Yellow,
                _ => Color::White,
            };

            let status_char = match file.status.as_str() {
                "added" => 'A',
                "removed" => 'D',
                "modified" => 'M',
                "renamed" => 'R',
                _ => '?',
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("[{}] ", status_char),
                    Style::default().fg(status_color),
                ),
                Span::styled(&file.filename, style),
                Span::raw(format!(" +{} -{}", file.additions, file.deletions)),
            ]);

            ListItem::new(line)
        })
        .collect()
}
