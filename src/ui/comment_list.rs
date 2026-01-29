use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthChar;

use super::common::render_rally_status_bar;
use crate::app::{App, CommentTab};

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

pub fn render(frame: &mut Frame, app: &mut App) {
    // Handle detail mode separately
    if app.discussion_comment_detail_mode {
        render_discussion_detail(frame, app);
        return;
    }

    let has_rally = app.has_background_rally();
    let constraints = if has_rally {
        vec![
            Constraint::Length(3), // Header with tabs
            Constraint::Min(0),    // Comment list
            Constraint::Length(1), // Rally status bar
            Constraint::Length(3), // Footer
        ]
    } else {
        vec![
            Constraint::Length(3), // Header with tabs
            Constraint::Min(0),    // Comment list
            Constraint::Length(3), // Footer
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    // Header with tabs
    render_tab_header(frame, app, chunks[0]);

    // Content based on active tab
    match app.comment_tab {
        CommentTab::Review => render_review_comments(frame, app, chunks[1]),
        CommentTab::Discussion => render_discussion_comments(frame, app, chunks[1]),
    }

    // Rally status bar (if background rally exists)
    if has_rally {
        render_rally_status_bar(frame, chunks[2], app);
    }

    // Footer
    let footer_chunk_idx = if has_rally { 3 } else { 2 };
    let footer_text = match app.comment_tab {
        CommentTab::Review => "j/k/↑↓: move | Enter: jump to file | [/]: switch tab | q: back",
        CommentTab::Discussion => "j/k/↑↓: move | Enter: view detail | [/]: switch tab | q: back",
    };
    let footer = Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[footer_chunk_idx]);
}

fn render_tab_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let review_count = app.review_comments.as_ref().map(|c| c.len()).unwrap_or(0);
    let discussion_count = app
        .discussion_comments
        .as_ref()
        .map(|c| c.len())
        .unwrap_or(0);

    let review_style = if app.comment_tab == CommentTab::Review {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let discussion_style = if app.comment_tab == CommentTab::Discussion {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let loading_indicator = |loading: bool| -> String {
        if loading {
            format!(" {}", app.spinner_char())
        } else {
            String::new()
        }
    };

    let header_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!(
                "[Review Comments ({})]{}",
                review_count,
                loading_indicator(app.comments_loading)
            ),
            review_style,
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "[Discussion ({})]{}",
                discussion_count,
                loading_indicator(app.discussion_comments_loading)
            ),
            discussion_style,
        ),
    ]);

    let header =
        Paragraph::new(header_line).block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, area);
}

fn render_review_comments(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    if app.comments_loading && app.review_comments.is_none() {
        let loading = Paragraph::new("Loading review comments...")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(loading, area);
        return;
    }

    let Some(ref comments) = app.review_comments else {
        let empty = Paragraph::new("No review comments")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, area);
        return;
    };

    if comments.is_empty() {
        let empty = Paragraph::new("No review comments found")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, area);
        return;
    }

    let available_width = area.width.saturating_sub(4) as usize;
    let body_width = available_width.saturating_sub(4);

    let items: Vec<ListItem> = comments
        .iter()
        .enumerate()
        .map(|(i, comment)| {
            let is_selected = i == app.selected_comment;
            let prefix = if is_selected { "> " } else { "  " };

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

            let body_text: String = comment.body.lines().collect::<Vec<_>>().join(" ");
            let wrapped_lines = wrap_text(&body_text, body_width);

            let mut lines = vec![header_line];
            for wrapped_line in wrapped_lines {
                lines.push(Line::from(vec![Span::raw("    "), Span::raw(wrapped_line)]));
            }
            lines.push(Line::from(""));

            ListItem::new(lines)
        })
        .collect();

    // Use ListState for stateful rendering with automatic scroll management
    let mut list_state = ListState::default()
        .with_offset(app.comment_list_scroll_offset)
        .with_selected(Some(app.selected_comment));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut list_state);

    // Update scroll offset from ListState for next frame
    app.comment_list_scroll_offset = list_state.offset();
}

fn render_discussion_comments(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    if app.discussion_comments_loading && app.discussion_comments.is_none() {
        let loading = Paragraph::new("Loading discussion comments...")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(loading, area);
        return;
    }

    let Some(ref comments) = app.discussion_comments else {
        let empty = Paragraph::new("No discussion comments")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, area);
        return;
    };

    if comments.is_empty() {
        let empty = Paragraph::new("No discussion comments found")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, area);
        return;
    }

    let available_width = area.width.saturating_sub(4) as usize;
    let body_width = available_width.saturating_sub(4);

    let items: Vec<ListItem> = comments
        .iter()
        .enumerate()
        .map(|(i, comment)| {
            let is_selected = i == app.selected_discussion_comment;
            let prefix = if is_selected { "> " } else { "  " };

            // Format created_at to a shorter form (just the date part)
            let date = comment
                .created_at
                .split('T')
                .next()
                .unwrap_or(&comment.created_at);

            let header_line = Line::from(vec![
                Span::raw(prefix),
                Span::styled(
                    format!("@{}", comment.user.login),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw("  "),
                Span::styled(date.to_string(), Style::default().fg(Color::DarkGray)),
            ]);

            // Truncate body for list view
            let body_text: String = comment.body.lines().collect::<Vec<_>>().join(" ");
            let truncated = if body_text.len() > body_width * 2 {
                format!("{}...", &body_text[..body_width * 2])
            } else {
                body_text
            };
            let wrapped_lines = wrap_text(&truncated, body_width);

            let mut lines = vec![header_line];
            for wrapped_line in wrapped_lines {
                lines.push(Line::from(vec![Span::raw("    "), Span::raw(wrapped_line)]));
            }
            lines.push(Line::from(""));

            ListItem::new(lines)
        })
        .collect();

    // Use ListState for stateful rendering with automatic scroll management
    let mut list_state = ListState::default()
        .with_offset(app.discussion_comment_list_scroll_offset)
        .with_selected(Some(app.selected_discussion_comment));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut list_state);

    // Update scroll offset from ListState for next frame
    app.discussion_comment_list_scroll_offset = list_state.offset();
}

fn render_discussion_detail(frame: &mut Frame, app: &App) {
    let Some(ref comments) = app.discussion_comments else {
        return;
    };
    let Some(comment) = comments.get(app.selected_discussion_comment) else {
        return;
    };

    let has_rally = app.has_background_rally();
    let constraints = if has_rally {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Rally status bar
            Constraint::Length(3), // Footer
        ]
    } else {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Footer
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    // Header
    let date = comment
        .created_at
        .split('T')
        .next()
        .unwrap_or(&comment.created_at);
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("@{}", comment.user.login),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::styled(date.to_string(), Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Comment Detail"),
    );
    frame.render_widget(header, chunks[0]);

    // Content with scroll
    let content_height = chunks[1].height.saturating_sub(2) as usize;
    let body_lines: Vec<Line> = comment
        .body
        .lines()
        .skip(app.discussion_comment_detail_scroll)
        .take(content_height)
        .map(|line| Line::from(line.to_string()))
        .collect();

    let total_lines = comment.body.lines().count();
    let scroll_info = if total_lines > content_height {
        format!(
            " ({}/{})",
            app.discussion_comment_detail_scroll + 1,
            total_lines.saturating_sub(content_height) + 1
        )
    } else {
        String::new()
    };

    let content = Paragraph::new(body_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Content{}", scroll_info)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(content, chunks[1]);

    // Rally status bar (if background rally exists)
    if has_rally {
        render_rally_status_bar(frame, chunks[2], app);
    }

    // Footer
    let footer_chunk_idx = if has_rally { 3 } else { 2 };
    let footer = Paragraph::new("j/k/↑↓: scroll | Ctrl+d/u: page | Enter/Esc: back to list")
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[footer_chunk_idx]);
}
