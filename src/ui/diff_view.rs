use std::collections::HashSet;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use syntect::easy::HighlightLines;

use super::common::render_rally_status_bar;
use crate::app::{App, CachedDiffLine};
use crate::diff::{classify_line, LineType};
use crate::syntax::{get_theme, highlight_code_line, syntax_for_file};

/// Build cached diff lines with syntax highlighting (called from App::ensure_diff_cache)
pub fn build_diff_cache(
    patch: &str,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
) -> Vec<CachedDiffLine> {
    let syntax = syntax_for_file(filename);
    let theme = get_theme(theme_name);
    let mut highlighter = syntax.map(|s| HighlightLines::new(s, theme));

    patch
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let has_comment = comment_lines.contains(&i);
            let (line_type, content) = classify_line(line);

            let mut spans = build_line_spans(line_type, line, content, &mut highlighter);

            // Add comment indicator at the beginning if this line has comments
            if has_comment {
                spans.insert(0, Span::styled("● ", Style::default().fg(Color::Yellow)));
            }

            CachedDiffLine { spans }
        })
        .collect()
}

/// Convert cached diff lines to renderable [`Line`]s using zero-copy borrowing.
///
/// Borrows span content from the cache (`Cow::Borrowed`) instead of cloning,
/// avoiding heap allocations entirely.
///
/// * `cached_lines` – slice of cached lines to render (may be a sub-range).
/// * `start_index`  – absolute index of the first element in `cached_lines`,
///   used to correctly identify the selected line.
/// * `selected_line` – absolute index of the currently selected line.
pub fn render_cached_lines<'a>(
    cached_lines: &'a [CachedDiffLine],
    start_index: usize,
    selected_line: usize,
) -> Vec<Line<'a>> {
    cached_lines
        .iter()
        .enumerate()
        .map(|(rel_idx, cached)| {
            let abs_idx = start_index + rel_idx;
            let is_selected = abs_idx == selected_line;
            let spans: Vec<Span<'_>> = cached
                .spans
                .iter()
                .map(|s| Span::styled(s.content.as_ref(), s.style))
                .collect();
            if is_selected {
                Line::from(spans).style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                Line::from(spans)
            }
        })
        .collect()
}

/// Render cached diff lines with cursor word highlight on the selected line.
///
/// Wraps `render_cached_lines` and applies `UNDERLINED` modifier to the word
/// under the cursor on the selected line.
pub fn render_cached_lines_with_cursor<'a>(
    cached_lines: &'a [CachedDiffLine],
    start_index: usize,
    selected_line: usize,
    cursor_column: usize,
    line_content: Option<&str>,
) -> Vec<Line<'a>> {
    cached_lines
        .iter()
        .enumerate()
        .map(|(rel_idx, cached)| {
            let abs_idx = start_index + rel_idx;
            let is_selected = abs_idx == selected_line;
            let spans: Vec<Span<'_>> = cached
                .spans
                .iter()
                .map(|s| Span::styled(s.content.as_ref(), s.style))
                .collect();

            if is_selected {
                // Apply word highlight if we have line content
                let highlighted_spans = if let Some(content) = line_content {
                    if let Some((_word, word_start, word_end)) =
                        crate::symbol::extract_word_at(content, cursor_column)
                    {
                        apply_word_highlight_spans(&spans, word_start, word_end, cached)
                    } else {
                        spans
                    }
                } else {
                    spans
                };
                Line::from(highlighted_spans)
                    .style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                Line::from(spans)
            }
        })
        .collect()
}

/// Apply underline highlight to the word under cursor within span list.
///
/// Calculates the content offset (skipping diff markers like +/-/space and
/// comment indicators) and splits spans at word boundaries to apply UNDERLINED.
/// Returns owned spans to avoid lifetime issues.
fn apply_word_highlight_spans<'a>(
    spans: &[Span<'a>],
    word_start: usize,
    word_end: usize,
    cached: &CachedDiffLine,
) -> Vec<Span<'static>> {
    // Calculate content offset: marker chars (+/-/space = 1 char) and optional
    // comment indicator ("● " = 2 chars)
    let mut content_offset = 0;
    let first_span_content = cached
        .spans
        .first()
        .map(|s| s.content.as_ref())
        .unwrap_or("");

    // Check if first span is a comment indicator
    if first_span_content == "● " {
        content_offset += 2; // comment indicator
        // Next span is the diff marker
        if cached.spans.len() > 1 {
            content_offset += cached.spans[1].content.chars().count();
        }
    } else {
        // First span is the diff marker (+/-/space)
        content_offset += first_span_content.chars().count();
    }

    let abs_start = content_offset + word_start;
    let abs_end = content_offset + word_end;

    // Walk through spans and split at word boundaries to apply underline
    let mut new_spans: Vec<Span<'static>> = Vec::new();
    let mut char_pos = 0;

    for span in spans {
        let span_text: &str = span.content.as_ref();
        let span_char_count = span_text.chars().count();
        let span_end = char_pos + span_char_count;

        if span_end <= abs_start || char_pos >= abs_end {
            // Span is entirely outside the word range
            new_spans.push(Span::styled(span_text.to_string(), span.style));
        } else {
            // Span overlaps with the word range - need to split
            let chars: Vec<char> = span_text.chars().collect();

            let local_start = abs_start.saturating_sub(char_pos);
            let local_end = if abs_end < span_end {
                abs_end - char_pos
            } else {
                span_char_count
            };

            // Before word
            if local_start > 0 {
                let before: String = chars[..local_start].iter().collect();
                new_spans.push(Span::styled(before, span.style));
            }

            // Word portion (underlined)
            let word_part: String = chars[local_start..local_end].iter().collect();
            new_spans.push(Span::styled(
                word_part,
                span.style.add_modifier(Modifier::UNDERLINED),
            ));

            // After word
            if local_end < span_char_count {
                let after: String = chars[local_end..].iter().collect();
                new_spans.push(Span::styled(after, span.style));
            }
        }

        char_pos = span_end;
    }

    new_spans
}

pub fn render(frame: &mut Frame, app: &App) {
    // If current line has inline comments, show split view with comment panel
    if app.has_comment_at_current_line() {
        render_with_inline_comment(frame, app);
        return;
    }

    let has_rally = app.has_background_rally();
    let constraints = if has_rally {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Diff content
            Constraint::Length(1), // Rally status bar
            Constraint::Length(3), // Footer
        ]
    } else {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Diff content
            Constraint::Length(3), // Footer
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_diff_content(frame, app, chunks[1]);

    // Rally status bar (if background rally exists)
    if has_rally {
        render_rally_status_bar(frame, chunks[2], app);
        render_footer(frame, app, chunks[3]);
    } else {
        render_footer(frame, app, chunks[2]);
    }
}

/// Render diff view with inline comment panel at bottom
fn render_with_inline_comment(frame: &mut Frame, app: &App) {
    let has_rally = app.has_background_rally();
    let constraints = if has_rally {
        vec![
            Constraint::Length(3),      // Header
            Constraint::Percentage(55), // Diff content
            Constraint::Length(1),      // Rally status bar
            Constraint::Percentage(40), // Inline comments
        ]
    } else {
        vec![
            Constraint::Length(3),      // Header
            Constraint::Percentage(60), // Diff content
            Constraint::Percentage(40), // Inline comments
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_diff_content(frame, app, chunks[1]);

    if has_rally {
        render_rally_status_bar(frame, chunks[2], app);
        render_inline_comments(frame, app, chunks[3]);
    } else {
        render_inline_comments(frame, app, chunks[2]);
    }
}

pub fn render_with_preview(frame: &mut Frame, app: &App) {
    let has_rally = app.has_background_rally();
    let constraints = if has_rally {
        vec![
            Constraint::Length(3),      // Header
            Constraint::Percentage(55), // Diff content (slightly reduced)
            Constraint::Length(1),      // Rally status bar
            Constraint::Percentage(40), // Comment preview
        ]
    } else {
        vec![
            Constraint::Length(3),      // Header
            Constraint::Percentage(60), // Diff content
            Constraint::Percentage(40), // Comment preview
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_diff_content(frame, app, chunks[1]);

    if has_rally {
        render_rally_status_bar(frame, chunks[2], app);
        render_comment_preview(frame, app, chunks[3]);
    } else {
        render_comment_preview(frame, app, chunks[2]);
    }
}

pub(crate) fn render_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
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

pub(crate) fn render_diff_content(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    // Determine if we should show cursor word highlight
    let in_diff_state = matches!(
        app.state,
        crate::app::AppState::DiffView | crate::app::AppState::SplitViewDiff
    );

    // Try to use cached lines if available
    let lines: Vec<Line> = if let Some(ref cache) = app.diff_cache {
        // Calculate visible range for optimization
        // Add buffer for smooth scrolling and wrap handling
        let visible_height = area.height.saturating_sub(2) as usize;
        let line_count = cache.lines.len();

        // Clamp visible_start to avoid out-of-bounds access when scroll_offset >= line_count
        let visible_start = app.scroll_offset.saturating_sub(2).min(line_count);
        let visible_end = (app.scroll_offset + visible_height + 5).min(line_count);

        // Only process visible lines (with buffer) for performance
        // When visible_start >= visible_end, this produces an empty slice (safe)
        if in_diff_state {
            render_cached_lines_with_cursor(
                &cache.lines[visible_start..visible_end],
                visible_start,
                app.selected_line,
                app.cursor_column,
                app.current_line_content.as_deref(),
            )
        } else {
            render_cached_lines(
                &cache.lines[visible_start..visible_end],
                visible_start,
                app.selected_line,
            )
        }
    } else {
        // Fallback: parse without cache (should rarely happen)
        let file = app.files().get(app.selected_file);
        let theme_name = &app.config.diff.theme;

        match file {
            Some(f) => match f.patch.as_ref() {
                Some(patch) => parse_patch_to_lines(
                    patch,
                    app.selected_line,
                    &f.filename,
                    theme_name,
                    &app.file_comment_lines,
                ),
                None => vec![Line::from("No diff available")],
            },
            None => vec![Line::from("No file selected")],
        }
    };

    // Adjust scroll offset for visible range processing
    let adjusted_scroll = if app.diff_cache.is_some() {
        let visible_start = app.scroll_offset.saturating_sub(2);
        (app.scroll_offset - visible_start) as u16
    } else {
        app.scroll_offset as u16
    };

    let diff_block = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false })
        .scroll((adjusted_scroll, 0));

    frame.render_widget(diff_block, area);
}

fn parse_patch_to_lines(
    patch: &str,
    selected_line: usize,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
) -> Vec<Line<'static>> {
    let syntax = syntax_for_file(filename);
    let theme = get_theme(theme_name);
    let mut highlighter = syntax.map(|s| HighlightLines::new(s, theme));

    patch
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let is_selected = i == selected_line;
            let has_comment = comment_lines.contains(&i);
            let (line_type, content) = classify_line(line);

            let mut spans = build_line_spans(line_type, line, content, &mut highlighter);

            // Add comment indicator at the beginning if this line has comments
            if has_comment {
                spans.insert(0, Span::styled("● ", Style::default().fg(Color::Yellow)));
            }

            if is_selected {
                for span in &mut spans {
                    span.style = span.style.add_modifier(Modifier::REVERSED);
                }
            }

            Line::from(spans)
        })
        .collect()
}

fn build_line_spans(
    line_type: LineType,
    original_line: &str,
    content: &str,
    highlighter: &mut Option<HighlightLines<'_>>,
) -> Vec<Span<'static>> {
    match line_type {
        LineType::Header => {
            vec![Span::styled(
                original_line.to_string(),
                Style::default().fg(Color::Cyan),
            )]
        }
        LineType::Meta => {
            vec![Span::styled(
                original_line.to_string(),
                Style::default().fg(Color::Yellow),
            )]
        }
        LineType::Added => {
            let marker = Span::styled("+", Style::default().fg(Color::Green));
            let code_spans = highlight_or_fallback(content, highlighter, Color::Green);
            std::iter::once(marker).chain(code_spans).collect()
        }
        LineType::Removed => {
            let marker = Span::styled("-", Style::default().fg(Color::Red));
            let code_spans = highlight_or_fallback(content, highlighter, Color::Red);
            std::iter::once(marker).chain(code_spans).collect()
        }
        LineType::Context => {
            let marker = Span::styled(" ", Style::default());
            let code_spans = highlight_or_fallback(content, highlighter, Color::Reset);
            std::iter::once(marker).chain(code_spans).collect()
        }
    }
}

fn highlight_or_fallback(
    content: &str,
    highlighter: &mut Option<HighlightLines<'_>>,
    fallback_color: Color,
) -> Vec<Span<'static>> {
    match highlighter {
        Some(h) => {
            let spans = highlight_code_line(content, h);
            if spans.is_empty() {
                // Empty content, return empty span
                vec![Span::raw(content.to_string())]
            } else {
                spans
            }
        }
        None => vec![Span::styled(
            content.to_string(),
            Style::default().fg(fallback_color),
        )],
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let help_text = "j/k: move | n/N: next/prev comment | c: comment | s: suggestion | Ctrl-d/u: page | q: back";

    // Build footer content with submission status
    let mut spans = vec![Span::raw(help_text)];

    if app.is_submitting_comment() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} Submitting...", app.spinner_char()),
            Style::default().fg(Color::Yellow),
        ));
    } else if let Some((success, message)) = &app.submission_result {
        spans.push(Span::raw("  "));
        if *success {
            spans.push(Span::styled(
                format!("✓ {}", message),
                Style::default().fg(Color::Green),
            ));
        } else {
            spans.push(Span::styled(
                format!("✗ {}", message),
                Style::default().fg(Color::Red),
            ));
        }
    } else if app.comments_loading {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} Loading comments...", app.spinner_char()),
            Style::default().fg(Color::Yellow),
        ));
    }

    let footer = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL));
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
    let has_rally = app.has_background_rally();
    let constraints = if has_rally {
        vec![
            Constraint::Length(3),      // Header
            Constraint::Percentage(45), // Diff content (slightly reduced)
            Constraint::Length(1),      // Rally status bar
            Constraint::Percentage(50), // Suggestion preview
        ]
    } else {
        vec![
            Constraint::Length(3),      // Header
            Constraint::Percentage(50), // Diff content
            Constraint::Percentage(50), // Suggestion preview
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    render_header(frame, app, chunks[0]);
    render_diff_content(frame, app, chunks[1]);

    if has_rally {
        render_rally_status_bar(frame, chunks[2], app);
        render_suggestion_preview(frame, app, chunks[3]);
    } else {
        render_suggestion_preview(frame, app, chunks[2]);
    }
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
                format!(
                    "```suggestion\n{}\n```",
                    suggestion.suggested_code.trim_end()
                ),
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

/// Render inline comments panel for current line
fn render_inline_comments(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let indices = app.get_comment_indices_at_current_line();
    let Some(ref comments) = app.review_comments else {
        return;
    };

    let mut lines: Vec<Line> = vec![];
    for (i, &idx) in indices.iter().enumerate() {
        let Some(comment) = comments.get(idx) else {
            continue;
        };

        if i > 0 {
            // Separator between multiple comments
            lines.push(Line::from(Span::styled(
                "───────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
        }

        // Header: @user (line N)
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

        // Body
        for line in comment.body.lines() {
            lines.push(Line::from(line.to_string()));
        }
        lines.push(Line::from("")); // Spacing after comment body
    }

    let title = format!(
        "Comment{} (n/N: jump)",
        if indices.len() > 1 { "s" } else { "" }
    );

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });

    frame.render_widget(paragraph, area);
}
