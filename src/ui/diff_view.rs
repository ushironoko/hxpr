use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Diff content
            Constraint::Length(3), // Footer
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_diff_content(frame, app, chunks[1]);
    render_footer(frame, chunks[2]);
}

pub fn render_with_preview(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Header
            Constraint::Percentage(60), // Diff content
            Constraint::Percentage(40), // Comment preview
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_diff_content(frame, app, chunks[1]);
    render_comment_preview(frame, app, chunks[2]);
}

fn render_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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

    let header =
        Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("Diff"));
    frame.render_widget(header, area);
}

fn render_diff_content(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let lines: Vec<Line> = app
        .files()
        .get(app.selected_file)
        .and_then(|file| file.patch.as_ref())
        .map(|patch| parse_patch_to_lines(patch, app.selected_line))
        .unwrap_or_else(|| vec![Line::from("No diff available")]);

    let diff_block = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset as u16, 0));

    frame.render_widget(diff_block, area);
}

fn parse_patch_to_lines(patch: &str, selected_line: usize) -> Vec<Line<'static>> {
    patch
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let is_selected = i == selected_line;
            let mut style = get_line_style(line);

            if is_selected {
                style = style.add_modifier(Modifier::REVERSED);
            }

            Line::from(vec![Span::styled(line.to_string(), style)])
        })
        .collect()
}

fn get_line_style(line: &str) -> Style {
    if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Cyan)
    } else if line.starts_with("diff ") || line.starts_with("index ") {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect) {
    let footer = Paragraph::new(
        "j/k: move | c: comment | s: suggestion | Ctrl-d/u: page down/up | q/Esc: back to list",
    )
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, area);
}

fn render_comment_preview(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let preview_lines: Vec<Line> = if let Some(ref comment) = app.pending_comment {
        vec![
            Line::from(vec![
                Span::styled("Line ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    comment.line_number.to_string(),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(""),
            Line::from(comment.body.as_str()),
        ]
    } else {
        vec![Line::from("No comment pending")]
    };

    let preview = Paragraph::new(preview_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Comment Preview (Enter: submit, Esc: cancel)"),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(preview, area);
}

pub fn render_with_suggestion_preview(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Header
            Constraint::Percentage(50), // Diff content
            Constraint::Percentage(50), // Suggestion preview
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_diff_content(frame, app, chunks[1]);
    render_suggestion_preview(frame, app, chunks[2]);
}

fn render_suggestion_preview(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let preview_lines: Vec<Line> = if let Some(ref suggestion) = app.pending_suggestion {
        vec![
            Line::from(vec![
                Span::styled("Line ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    suggestion.line_number.to_string(),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Original:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                format!("  {}", suggestion.original_code),
                Style::default().fg(Color::Red),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Suggested:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                format!("  {}", suggestion.suggested_code.trim_end()),
                Style::default().fg(Color::Green),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Will be posted as:",
                Style::default().fg(Color::DarkGray),
            )]),
            Line::from(vec![Span::styled(
                format!("```suggestion\n{}\n```", suggestion.suggested_code.trim_end()),
                Style::default().fg(Color::White),
            )]),
        ]
    } else {
        vec![Line::from("No suggestion pending")]
    };

    let preview = Paragraph::new(preview_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Suggestion Preview (Enter: submit, Esc: cancel)"),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(preview, area);
}
