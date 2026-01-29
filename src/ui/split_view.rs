use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, Paragraph, Wrap},
    Frame,
};

use super::common::render_rally_status_bar;
use super::diff_view;
use super::file_list::build_file_list_items;
use crate::app::{App, AppState, DataState};

pub fn render(frame: &mut Frame, app: &App) {
    let has_rally = app.has_background_rally();

    // Rally status bar の有無で垂直分割
    let outer_constraints = if has_rally {
        vec![
            Constraint::Min(0),    // Main content
            Constraint::Length(1), // Rally status bar
        ]
    } else {
        vec![Constraint::Min(0)]
    };

    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(outer_constraints)
        .split(frame.area());

    // 横並びレイアウト: 左35% / 右65%
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer_chunks[0]);

    let is_file_focused = app.state == AppState::SplitViewFileList;
    let is_diff_focused = app.state == AppState::SplitViewDiff;

    render_file_list_pane(frame, app, h_chunks[0], is_file_focused);
    render_diff_pane(frame, app, h_chunks[1], is_diff_focused);

    // Rally status bar
    if has_rally {
        render_rally_status_bar(frame, outer_chunks[1], app);
    }
}

fn render_file_list_pane(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    is_focused: bool,
) {
    let border_color = if is_focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),   // File list
            Constraint::Length(3), // Footer
        ])
        .split(area);

    // Header
    let pr_info = match &app.data_state {
        DataState::Loaded { pr, .. } => {
            format!("PR #{}: {}", pr.number, pr.title)
        }
        _ => format!("PR #{}", app.pr_number),
    };

    let header = Paragraph::new(pr_info).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title("octorus"),
    );
    frame.render_widget(header, chunks[0]);

    // File list
    let files = app.files();
    let items = build_file_list_items(files, app.selected_file);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(format!("Files ({})", files.len())),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_widget(list, chunks[1]);

    // Footer
    let footer_text = if is_focused {
        "j/k: move | Enter/→/l: diff | ←/h/q: back"
    } else {
        "←/h: focus files"
    };
    let footer = Paragraph::new(footer_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );
    frame.render_widget(footer, chunks[2]);
}

fn render_diff_pane(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    is_focused: bool,
) {
    let border_color = if is_focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    // コメントパネルが開いている場合は分割表示
    let has_inline_comment = is_focused && app.comment_panel_open;

    if has_inline_comment {
        render_diff_pane_with_comments(frame, app, area, border_color);
    } else {
        render_diff_pane_normal(frame, app, area, border_color, is_focused);
    }
}

fn render_diff_pane_normal(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    border_color: Color,
    is_focused: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),   // Diff content
            Constraint::Length(3), // Footer
        ])
        .split(area);

    render_diff_header(frame, app, chunks[0], border_color);
    render_diff_body(frame, app, chunks[1], border_color);

    // Footer
    let footer_text = if is_focused {
        "j/k: scroll | n/N: next/prev comment | Enter: comments | →/l: fullscreen | ←/h: files | q: back"
    } else {
        "Enter/→: focus diff"
    };

    render_diff_footer(frame, app, chunks[2], footer_text, border_color);
}

fn render_diff_footer(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    help_text: &str,
    border_color: Color,
) {
    let mut footer_spans = vec![Span::raw(help_text.to_string())];

    if app.is_submitting_comment() {
        footer_spans.push(Span::raw("  "));
        footer_spans.push(Span::styled(
            format!("{} Submitting...", app.spinner_char()),
            Style::default().fg(Color::Yellow),
        ));
    } else if let Some((success, message)) = &app.submission_result {
        footer_spans.push(Span::raw("  "));
        if *success {
            footer_spans.push(Span::styled(
                format!("✓ {}", message),
                Style::default().fg(Color::Green),
            ));
        } else {
            footer_spans.push(Span::styled(
                format!("✗ {}", message),
                Style::default().fg(Color::Red),
            ));
        }
    } else if app.comments_loading {
        footer_spans.push(Span::raw("  "));
        footer_spans.push(Span::styled(
            format!("{} Loading comments...", app.spinner_char()),
            Style::default().fg(Color::Yellow),
        ));
    }

    let footer = Paragraph::new(Line::from(footer_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    );
    frame.render_widget(footer, area);
}

fn render_diff_pane_with_comments(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    border_color: Color,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Header
            Constraint::Percentage(50), // Diff content
            Constraint::Percentage(40), // Inline comments
            Constraint::Length(3),      // Footer
        ])
        .split(area);

    render_diff_header(frame, app, chunks[0], border_color);
    render_diff_body(frame, app, chunks[1], border_color);

    // Inline comments
    let indices = app.get_comment_indices_at_current_line();
    let mut lines: Vec<Line> = vec![];

    if indices.is_empty() {
        lines.push(Line::from(Span::styled(
            "No comments. c: comment, s: suggestion",
            Style::default().fg(Color::DarkGray),
        )));
    } else if let Some(ref comments) = app.review_comments {
        for (i, &idx) in indices.iter().enumerate() {
            let Some(comment) = comments.get(idx) else {
                continue;
            };

            if i > 0 {
                lines.push(Line::from(Span::styled(
                    "───────────────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }

            lines.push(Line::from(vec![
                Span::styled(
                    format!("@{}", comment.user.login),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!(" (line {})", comment.line.unwrap_or(0)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            for line in comment.body.lines() {
                lines.push(Line::from(line.to_string()));
            }
            lines.push(Line::from(""));
        }
    }

    let title = "Comments (j/k: scroll, c: comment, s: suggest, r: reply)";

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(title),
        )
        .wrap(Wrap { trim: true })
        .scroll((app.comment_panel_scroll, 0));
    frame.render_widget(paragraph, chunks[2]);

    // Footer
    let footer_text = "j/k: scroll | n/N: jump | Tab: switch | r: reply | c: comment | s: suggest | →/l: fullscreen | ←/h/q: close";
    render_diff_footer(frame, app, chunks[3], footer_text, border_color);
}

fn render_diff_header(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    border_color: Color,
) {
    let header_text = app
        .files()
        .get(app.selected_file)
        .map(|file| {
            format!(
                "{} (+{} -{})",
                file.filename, file.additions, file.deletions
            )
        })
        .unwrap_or_else(|| "No file selected".to_string());

    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title("Diff Preview"),
    );
    frame.render_widget(header, area);
}

fn render_diff_body(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    border_color: Color,
) {
    let lines: Vec<Line> = if let Some(ref cache) = app.diff_cache {
        let visible_height = area.height.saturating_sub(2) as usize;
        let line_count = cache.lines.len();
        let visible_start = app.scroll_offset.saturating_sub(2).min(line_count);
        let visible_end = (app.scroll_offset + visible_height + 5).min(line_count);

        diff_view::render_cached_lines(
            &cache.lines[visible_start..visible_end],
            visible_start,
            app.selected_line,
        )
    } else {
        let file = app.files().get(app.selected_file);
        match file {
            Some(f) => match f.patch.as_ref() {
                Some(_) => vec![Line::from("Loading diff...")],
                None => vec![Line::from("No diff available")],
            },
            None => vec![Line::from("No file selected")],
        }
    };

    let adjusted_scroll = if app.diff_cache.is_some() {
        let visible_start = app.scroll_offset.saturating_sub(2);
        (app.scroll_offset - visible_start) as u16
    } else {
        app.scroll_offset as u16
    };

    let diff_block = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        )
        .wrap(Wrap { trim: false })
        .scroll((adjusted_scroll, 0));

    frame.render_widget(diff_block, area);
}
