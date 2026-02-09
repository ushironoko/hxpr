use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

use super::common::render_rally_status_bar;
use crate::app::{App, DataState};
use crate::github::ChangedFile;

pub fn render(frame: &mut Frame, app: &mut App) {
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
        _ => match app.pr_number {
            Some(n) => format!("PR #{}", n),
            None => "PR".to_string(),
        },
    };

    let header =
        Paragraph::new(pr_info).block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // File list
    let files = app.files();
    let total_files = files.len();
    let items = build_file_list_items(files, app.selected_file);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Changed Files ({})", total_files)),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    let mut list_state = ListState::default()
        .with_offset(app.file_list_scroll_offset)
        .with_selected(Some(app.selected_file));

    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    // Persist both offset and clamped selected index from ListState
    // (render_stateful_widget may clamp selected if list shrank)
    app.file_list_scroll_offset = list_state.offset();
    if let Some(sel) = list_state.selected() {
        app.selected_file = sel;
    }

    // Render scrollbar if there are more files than visible
    if total_files > 1 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

        let mut scrollbar_state =
            ScrollbarState::new(total_files.saturating_sub(1)).position(app.selected_file);

        frame.render_stateful_widget(
            scrollbar,
            chunks[1].inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }

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
        "j/k/↑↓: move | Enter/→/l: split view | a: approve | r: request changes | c: comment | C: comments | {} | R: refresh | q: quit | ?: help",
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
    let header_text = match app.pr_number {
        Some(n) => format!("PR #{} - Loading...", n),
        None => "Loading...".to_string(),
    };
    let header =
        Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("octorus"));
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
    let header_text = match app.pr_number {
        Some(n) => format!("PR #{} - Error", n),
        None => "Error".to_string(),
    };
    let header =
        Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("octorus"));
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
