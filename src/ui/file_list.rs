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

use super::common::{build_pr_info, render_rally_status_bar};
use crate::app::App;
use crate::github::ChangedFile;

pub fn render(frame: &mut Frame, app: &mut App) {
    let has_rally = app.has_background_rally();
    let has_filter_bar = app
        .file_list_filter
        .as_ref()
        .is_some_and(|f| f.input_active);

    let mut constraints = vec![
        Constraint::Length(3), // Header
        Constraint::Min(0),    // File list
    ];
    if has_filter_bar {
        constraints.push(Constraint::Length(3)); // Filter bar
    }
    if has_rally {
        constraints.push(Constraint::Length(1)); // Rally status bar
    }
    constraints.push(Constraint::Length(3)); // Footer

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    // Header
    let pr_info = build_pr_info(app);

    let header =
        Paragraph::new(pr_info).block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // File list
    let files = app.files();
    let total_files = files.len();

    // フィルタ適用中はフィルタ済みサブセットを表示
    if let Some(ref filter) = app.file_list_filter {
        if filter.matched_indices.is_empty() {
            // マッチ0件
            let empty_msg = format!("No matches for '{}'", filter.query);
            let empty = Paragraph::new(empty_msg)
                .style(Style::default().fg(Color::DarkGray))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Changed Files (0/{})", total_files)),
                );
            frame.render_widget(empty, chunks[1]);
        } else {
            let filtered: Vec<&ChangedFile> = filter
                .matched_indices
                .iter()
                .map(|&i| &files[i])
                .collect();
            let display_selected = filter.selected.unwrap_or(0);
            let display_count = filtered.len();

            let items = build_file_list_items_ref(&filtered, display_selected);

            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Changed Files ({}/{})", display_count, total_files)),
                )
                .highlight_style(Style::default().bg(Color::DarkGray));

            let mut list_state = ListState::default()
                .with_offset(app.file_list_scroll_offset)
                .with_selected(Some(display_selected));

            frame.render_stateful_widget(list, chunks[1], &mut list_state);

            app.file_list_scroll_offset = list_state.offset();

            if display_count > 1 {
                let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("▲"))
                    .end_symbol(Some("▼"));

                let mut scrollbar_state = ScrollbarState::new(display_count.saturating_sub(1))
                    .position(display_selected);

                frame.render_stateful_widget(
                    scrollbar,
                    chunks[1].inner(Margin {
                        vertical: 1,
                        horizontal: 0,
                    }),
                    &mut scrollbar_state,
                );
            }
        }
    } else {
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
    }

    // Track chunk index for remaining elements
    let mut next_chunk = 2;

    // Filter bar
    if has_filter_bar {
        if let Some(ref filter) = app.file_list_filter {
            render_filter_bar(frame, chunks[next_chunk], filter);
        }
        next_chunk += 1;
    }

    // Rally status bar (if background rally exists)
    if has_rally {
        render_rally_status_bar(frame, chunks[next_chunk], app);
        next_chunk += 1;
    }

    // Footer (dynamic based on rally state)
    let ai_rally_text = if app.has_background_rally() {
        "A: Resume Rally"
    } else {
        "A: AI Rally"
    };
    let filter_hint = if app.file_list_filter.is_some() {
        "Esc: clear filter"
    } else {
        "Space /: filter"
    };
    let help_text = if app.is_local_mode() {
        format!(
            "j/k/↑↓: move | Enter/→/l: split view | {} | {} | R: refresh | q: quit | ?: help",
            filter_hint, ai_rally_text
        )
    } else {
        format!(
            "j/k/↑↓: move | Enter/→/l: split view | {} | v: viewed | V: viewed dir | O: browser | a: approve | r: request changes | c: comment | C: comments | {} | R: refresh | q: quit | ?: help",
            filter_hint, ai_rally_text
        )
    };
    let footer_line = super::footer::build_footer_line(app, &help_text);
    let footer = Paragraph::new(footer_line).block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[next_chunk]);
}

fn render_filter_bar(frame: &mut Frame, area: ratatui::layout::Rect, filter: &crate::filter::ListFilter) {
    let cursor_display = format!("/{}", filter.query);
    let filter_bar = Paragraph::new(Line::from(vec![
        Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
        Span::styled(cursor_display, Style::default().fg(Color::White)),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
    frame.render_widget(filter_bar, area);
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
    let header_text = if app.is_local_mode() {
        let af = if app.is_local_auto_focus() { " AF" } else { "" };
        format!("[LOCAL{}] Loading...", af)
    } else {
        match app.pr_number {
            Some(n) => format!("PR #{} - Loading...", n),
            None => "Loading...".to_string(),
        }
    };
    let header =
        Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // Loading message
    let loading_msg = if app.is_local_mode() {
        format!("{} Loading local diff...", app.spinner_char())
    } else {
        format!("{} Loading PR data...", app.spinner_char())
    };
    let loading = Paragraph::new(loading_msg)
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
    let header_text = if app.is_local_mode() {
        "[LOCAL] Error".to_string()
    } else {
        match app.pr_number {
            Some(n) => format!("PR #{} - Error", n),
            None => "Error".to_string(),
        }
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
        .map(|(i, file)| build_file_list_item(file, i == selected_file))
        .collect()
}

/// フィルタ済みファイル一覧のリストアイテムを構築する
fn build_file_list_items_ref<'a>(
    files: &[&'a ChangedFile],
    selected: usize,
) -> Vec<ListItem<'a>> {
    files
        .iter()
        .enumerate()
        .map(|(i, file)| build_file_list_item(file, i == selected))
        .collect()
}

fn build_file_list_item<'a>(file: &'a ChangedFile, is_selected: bool) -> ListItem<'a> {
    let style = if is_selected {
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
        "copied" => Color::Cyan,
        _ => Color::White,
    };

    let status_char = match file.status.as_str() {
        "added" => 'A',
        "removed" => 'D',
        "modified" => 'M',
        "renamed" => 'R',
        "copied" => 'C',
        _ => '?',
    };

    let line = Line::from(vec![
        Span::styled(
            format!("[{}] ", status_char),
            Style::default().fg(status_color),
        ),
        if file.viewed {
            Span::styled("✓ ", Style::default().fg(Color::Green))
        } else {
            Span::raw("  ")
        },
        Span::styled(&file.filename, style),
        Span::raw(format!(" +{} -{}", file.additions, file.deletions)),
    ]);

    ListItem::new(line)
}
