use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthChar;

use crate::app::App;

/// Wrap text to fit within the specified width, handling multibyte characters
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for ch in text.chars() {
        // Skip newlines, treat as space
        if ch == '\n' {
            continue;
        }

        let char_width = ch.width().unwrap_or(1);

        if current_width + char_width > max_width {
            // Start new line
            lines.push(current_line);
            current_line = String::new();
            current_width = 0;
        }

        current_line.push(ch);
        current_width += char_width;
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Comment list
            Constraint::Length(3), // Footer
        ])
        .split(frame.area());

    // Header
    let comment_count = app.review_comments.as_ref().map(|c| c.len()).unwrap_or(0);
    let header = Paragraph::new(format!("Review Comments ({})", comment_count))
        .block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // Comment list
    if app.comments_loading {
        let loading = Paragraph::new("Loading comments...")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(loading, chunks[1]);
    } else if let Some(ref comments) = app.review_comments {
        if comments.is_empty() {
            let empty = Paragraph::new("No review comments found")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(empty, chunks[1]);
        } else {
            // Calculate available width for text (subtract borders and indent)
            let available_width = chunks[1].width.saturating_sub(4) as usize; // 2 for borders + 4 for indent
            let body_width = available_width.saturating_sub(4); // 4 spaces indent for body

            let items: Vec<ListItem> = comments
                .iter()
                .enumerate()
                .map(|(i, comment)| {
                    let is_selected = i == app.selected_comment;
                    let prefix = if is_selected { "> " } else { "  " };

                    let style = if is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    // First line: author, file, line
                    let line_info = comment.line.map(|l| format!(":{}", l)).unwrap_or_default();
                    let header_line = Line::from(vec![
                        Span::raw(prefix),
                        Span::styled(
                            format!("@{}", comment.user.login),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::raw(" on "),
                        Span::styled(
                            format!("{}{}", comment.path, line_info),
                            Style::default().fg(Color::Green),
                        ),
                    ]);

                    // Wrap comment body to multiple lines
                    let body_text: String = comment.body.lines().collect::<Vec<_>>().join(" ");
                    let wrapped_lines = wrap_text(&body_text, body_width);

                    let mut lines = vec![header_line];
                    for wrapped_line in wrapped_lines {
                        lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(wrapped_line, style),
                        ]));
                    }
                    lines.push(Line::from(""));

                    ListItem::new(lines)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().bg(Color::DarkGray));
            frame.render_widget(list, chunks[1]);
        }
    }

    // Footer
    let footer = Paragraph::new("j/k: move | Enter: jump to file | q/Esc: back")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}
