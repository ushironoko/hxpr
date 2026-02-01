use std::collections::HashSet;

use lasso::Rodeo;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};
use syntect::easy::HighlightLines;

use super::common::render_rally_status_bar;
use crate::app::{
    hash_string, App, CachedDiffLine, DiffCache, InputMode, InternedSpan, LineInputContext,
};
use crate::diff::{classify_line, LineType};
use crate::syntax::{
    get_theme, highlight_code_line, highlight_line_with_tree, syntax_for_file, Highlighter,
    ParserPool,
};

/// Build DiffCache with syntax highlighting and string interning.
///
/// Uses tree-sitter for supported languages (Rust, TypeScript, JavaScript, Go, Python)
/// and falls back to syntect for other languages.
///
/// Returns a complete DiffCache with file_index set to 0 (caller should update).
pub fn build_diff_cache(
    patch: &str,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
) -> DiffCache {
    let mut interner = Rodeo::default();
    let mut parser_pool = ParserPool::new();
    let mut highlighter = Highlighter::for_file(filename, theme_name, &mut parser_pool);

    // Try to build a combined source for CST highlighting
    // For CST, we need to parse the entire content at once
    let combined_source = build_combined_source_for_highlight(patch);
    let cst_result = highlighter.parse_source(&combined_source);

    let lines: Vec<CachedDiffLine> = if let Some(ref result) = cst_result {
        // CST path: use tree-sitter with full AST context
        build_lines_with_cst(
            patch,
            comment_lines,
            &combined_source,
            result,
            &mut interner,
        )
    } else {
        // Syntect fallback path
        build_lines_with_syntect(patch, filename, theme_name, comment_lines, &mut interner)
    };

    DiffCache {
        file_index: 0, // Caller should update this
        patch_hash: hash_string(patch),
        comment_lines: comment_lines.clone(),
        lines,
        interner,
    }
}

/// Build combined source for CST highlighting by extracting code content from diff.
///
/// This strips diff markers (+/-/ ) and hunk headers to create pure source code
/// that tree-sitter can parse correctly.
fn build_combined_source_for_highlight(patch: &str) -> String {
    let mut source = String::new();
    for line in patch.lines() {
        let (line_type, content) = classify_line(line);
        match line_type {
            // Include added, removed, and context lines in the source
            LineType::Added | LineType::Removed | LineType::Context => {
                source.push_str(content);
                source.push('\n');
            }
            // Skip headers and meta lines
            LineType::Header | LineType::Meta => {}
        }
    }
    source
}

/// Build cached lines using CST highlighting.
fn build_lines_with_cst(
    patch: &str,
    comment_lines: &HashSet<usize>,
    combined_source: &str,
    cst_result: &crate::syntax::CstParseResult,
    interner: &mut Rodeo,
) -> Vec<CachedDiffLine> {
    let mut source_byte_offset = 0;
    let mut source_lines = combined_source.lines().peekable();

    patch
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let has_comment = comment_lines.contains(&i);
            let (line_type, content) = classify_line(line);

            let spans = match line_type {
                LineType::Header => {
                    vec![InternedSpan {
                        content: interner.get_or_intern(line),
                        style: Style::default().fg(Color::Cyan),
                    }]
                }
                LineType::Meta => {
                    vec![InternedSpan {
                        content: interner.get_or_intern(line),
                        style: Style::default().fg(Color::Yellow),
                    }]
                }
                LineType::Added | LineType::Removed | LineType::Context => {
                    // Get the corresponding source line
                    let source_line = source_lines.next().unwrap_or("");
                    let line_start = source_byte_offset;
                    let line_end = source_byte_offset + source_line.len();
                    source_byte_offset = line_end + 1; // +1 for newline

                    let marker_style = match line_type {
                        LineType::Added => Style::default().fg(Color::Green),
                        LineType::Removed => Style::default().fg(Color::Red),
                        _ => Style::default(),
                    };

                    let marker = match line_type {
                        LineType::Added => "+",
                        LineType::Removed => "-",
                        LineType::Context => " ",
                        _ => "",
                    };

                    let mut spans = vec![InternedSpan {
                        content: interner.get_or_intern(marker),
                        style: marker_style,
                    }];

                    // Highlight the content using CST
                    let code_spans = highlight_line_with_tree(
                        combined_source,
                        content,
                        line_start,
                        line_end,
                        &cst_result.tree,
                        &cst_result.query,
                        &cst_result.capture_names,
                        interner,
                    );
                    spans.extend(code_spans);
                    spans
                }
            };

            let mut result_spans = spans;

            // Add comment indicator at the beginning if this line has comments
            if has_comment {
                let marker = interner.get_or_intern("● ");
                result_spans.insert(
                    0,
                    InternedSpan {
                        content: marker,
                        style: Style::default().fg(Color::Yellow),
                    },
                );
            }

            CachedDiffLine {
                spans: result_spans,
            }
        })
        .collect()
}

/// Build cached lines using syntect highlighting (fallback).
fn build_lines_with_syntect(
    patch: &str,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
    interner: &mut Rodeo,
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

            let mut spans = build_line_spans(line_type, line, content, &mut highlighter, interner);

            // Add comment indicator at the beginning if this line has comments
            if has_comment {
                let marker = interner.get_or_intern("● ");
                spans.insert(
                    0,
                    InternedSpan {
                        content: marker,
                        style: Style::default().fg(Color::Yellow),
                    },
                );
            }

            CachedDiffLine { spans }
        })
        .collect()
}

/// Convert cached diff lines to renderable [`Line`]s using zero-copy borrowing.
///
/// Resolves interned strings from the DiffCache's interner, avoiding heap
/// allocations entirely.
///
/// * `cache` – the DiffCache containing both lines and the interner.
/// * `range` – the range of lines to render (may be a sub-range).
/// * `selected_line` – absolute index of the currently selected line.
pub fn render_cached_lines<'a>(
    cache: &'a DiffCache,
    range: std::ops::Range<usize>,
    selected_line: usize,
) -> Vec<Line<'a>> {
    cache.lines[range.clone()]
        .iter()
        .enumerate()
        .map(|(rel_idx, cached)| {
            let abs_idx = range.start + rel_idx;
            let is_selected = abs_idx == selected_line;
            let spans: Vec<Span<'_>> = cached
                .spans
                .iter()
                .map(|s| Span::styled(cache.resolve(s.content), s.style))
                .collect();
            if is_selected {
                Line::from(spans).style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                Line::from(spans)
            }
        })
        .collect()
}

pub fn render(frame: &mut Frame, app: &App) {
    // If comment panel is open (focused), show split view with comment panel
    if app.comment_panel_open {
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
            Constraint::Percentage(50), // Diff content
            Constraint::Length(1),      // Rally status bar
            Constraint::Percentage(40), // Inline comments
            Constraint::Length(3),      // Footer
        ]
    } else {
        vec![
            Constraint::Length(3),      // Header
            Constraint::Percentage(55), // Diff content
            Constraint::Percentage(40), // Inline comments
            Constraint::Length(3),      // Footer
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
        render_footer(frame, app, chunks[4]);
    } else {
        render_inline_comments(frame, app, chunks[2]);
        render_footer(frame, app, chunks[3]);
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
    let visible_height = area.height.saturating_sub(2) as usize;

    // Try to use cached lines if available
    let mut lines: Vec<Line> = if let Some(ref cache) = app.diff_cache {
        // Calculate visible range for optimization
        // Add buffer for smooth scrolling and wrap handling
        let line_count = cache.lines.len();

        // Clamp visible_start to avoid out-of-bounds access when scroll_offset >= line_count
        let visible_start = app.scroll_offset.saturating_sub(2).min(line_count);
        let visible_end = (app.scroll_offset + visible_height + 5).min(line_count);

        // Only process visible lines (with buffer) for performance
        // When visible_start >= visible_end, this produces an empty range (safe)
        render_cached_lines(cache, visible_start..visible_end, app.selected_line)
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

    // Add bottom padding for scrolling past the last line
    let padding = visible_height / 2;
    for _ in 0..padding {
        lines.push(Line::from(""));
    }

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

    // Render scrollbar for diff content
    if let Some(ref cache) = app.diff_cache {
        let total_lines = cache.lines.len();
        let visible_height = area.height.saturating_sub(2) as usize;
        let max_scroll = total_lines.saturating_sub(visible_height);
        if max_scroll > 0 {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            let clamped_position = app.scroll_offset.min(max_scroll);
            let mut scrollbar_state = ScrollbarState::new(max_scroll).position(clamped_position);

            frame.render_stateful_widget(
                scrollbar,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }
}

fn parse_patch_to_lines(
    patch: &str,
    selected_line: usize,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
) -> Vec<Line<'static>> {
    // Build DiffCache and then convert to Lines
    // This ensures consistent behavior with cached path
    let cache = build_diff_cache(patch, filename, theme_name, comment_lines);

    cache
        .lines
        .iter()
        .enumerate()
        .map(|(i, cached)| {
            let is_selected = i == selected_line;
            let spans: Vec<Span<'static>> = cached
                .spans
                .iter()
                .map(|s| {
                    let text = cache.resolve(s.content).to_string();
                    let mut style = s.style;
                    if is_selected {
                        style = style.add_modifier(Modifier::REVERSED);
                    }
                    Span::styled(text, style)
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn build_line_spans(
    line_type: LineType,
    original_line: &str,
    content: &str,
    highlighter: &mut Option<HighlightLines<'_>>,
    interner: &mut Rodeo,
) -> Vec<InternedSpan> {
    match line_type {
        LineType::Header => {
            vec![InternedSpan {
                content: interner.get_or_intern(original_line),
                style: Style::default().fg(Color::Cyan),
            }]
        }
        LineType::Meta => {
            vec![InternedSpan {
                content: interner.get_or_intern(original_line),
                style: Style::default().fg(Color::Yellow),
            }]
        }
        LineType::Added => {
            let marker = InternedSpan {
                content: interner.get_or_intern("+"),
                style: Style::default().fg(Color::Green),
            };
            let code_spans = highlight_or_fallback(content, highlighter, Color::Green, interner);
            std::iter::once(marker).chain(code_spans).collect()
        }
        LineType::Removed => {
            let marker = InternedSpan {
                content: interner.get_or_intern("-"),
                style: Style::default().fg(Color::Red),
            };
            let code_spans = highlight_or_fallback(content, highlighter, Color::Red, interner);
            std::iter::once(marker).chain(code_spans).collect()
        }
        LineType::Context => {
            let marker = InternedSpan {
                content: interner.get_or_intern(" "),
                style: Style::default(),
            };
            let code_spans = highlight_or_fallback(content, highlighter, Color::Reset, interner);
            std::iter::once(marker).chain(code_spans).collect()
        }
    }
}

fn highlight_or_fallback(
    content: &str,
    highlighter: &mut Option<HighlightLines<'_>>,
    fallback_color: Color,
    interner: &mut Rodeo,
) -> Vec<InternedSpan> {
    match highlighter {
        Some(h) => {
            let spans = highlight_code_line(content, h, interner);
            if spans.is_empty() {
                // Empty content, return empty span
                vec![InternedSpan {
                    content: interner.get_or_intern(content),
                    style: Style::default(),
                }]
            } else {
                spans
            }
        }
        None => vec![InternedSpan {
            content: interner.get_or_intern(content),
            style: Style::default().fg(fallback_color),
        }],
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let help_text = if app.comment_panel_open {
        "j/k/↑↓: scroll | n/N: jump | Tab: switch | r: reply | c: comment | s: suggest | ←/h: back | Esc/q: close"
    } else {
        "j/k/↑↓: move | n/N: next/prev comment | Enter: comments | Ctrl-d/u: page | ←/h/q: back"
    };

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

/// Render inline comments panel for current line
fn render_inline_comments(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let indices = app.get_comment_indices_at_current_line();

    let mut lines: Vec<Line> = vec![];

    if indices.is_empty() {
        // コメントなしの場合
        lines.push(Line::from(Span::styled(
            "No comments. c: comment, s: suggestion",
            Style::default().fg(Color::DarkGray),
        )));
    } else if let Some(ref comments) = app.review_comments {
        let has_multiple = indices.len() > 1;

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

            // Selection indicator for multiple comments
            let indicator = if has_multiple {
                if i == app.selected_inline_comment {
                    Span::styled("> ", Style::default().fg(Color::Yellow))
                } else {
                    Span::styled("  ", Style::default())
                }
            } else {
                Span::raw("")
            };

            // Header: [>] @user (line N)
            lines.push(Line::from(vec![
                indicator,
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
    }

    let title = "Comments (j/k/↑↓: scroll, c: comment, s: suggest, r: reply)";
    let total_lines = lines.len();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(title);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .scroll((app.comment_panel_scroll, 0));

    frame.render_widget(paragraph, area);

    // Render scrollbar if there is content
    if total_lines > 1 {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

        let max_scroll = total_lines.saturating_sub(1);
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll).position(app.comment_panel_scroll as usize);

        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

/// Render unified text input view (comment/suggestion/reply)
pub fn render_text_input(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // Header
            Constraint::Percentage(40), // Context info area
            Constraint::Percentage(60), // TextArea
        ])
        .split(frame.area());

    render_header(frame, app, chunks[0]);

    match &app.input_mode {
        Some(InputMode::Comment(ctx)) => {
            render_comment_context(frame, app, chunks[1], ctx);
            render_comment_input_area(frame, app, chunks[2]);
        }
        Some(InputMode::Suggestion {
            context,
            original_code,
        }) => {
            render_suggestion_context(frame, app, chunks[1], context, original_code);
            render_suggestion_input_area(frame, app, chunks[2]);
        }
        Some(InputMode::Reply {
            reply_to_user,
            reply_to_body,
            ..
        }) => {
            render_reply_context(frame, chunks[1], reply_to_user, reply_to_body);
            render_reply_input_area(frame, app, chunks[2]);
        }
        None => {}
    }
}

/// Render context info for comment input
fn render_comment_context(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    ctx: &LineInputContext,
) {
    let filename = app
        .files()
        .get(ctx.file_index)
        .map(|f| f.filename.as_str())
        .unwrap_or("Unknown file");

    let lines = vec![
        Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(filename, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Line: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                ctx.line_number.to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Comment Location"),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Render TextArea for comment input
fn render_comment_input_area(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let submit_key = app.input_text_area.submit_key_display();
    let title = format!("Comment ({}: submit, Esc: cancel)", submit_key);
    app.input_text_area
        .render_with_title(frame, area, &title, "Type your comment here...");
}

/// Render context info for suggestion input
fn render_suggestion_context(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    ctx: &LineInputContext,
    original_code: &str,
) {
    let filename = app
        .files()
        .get(ctx.file_index)
        .map(|f| f.filename.as_str())
        .unwrap_or("Unknown file");

    let mut lines = vec![
        Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(filename, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Line: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                ctx.line_number.to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Original code:",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            format!("  {}", original_code),
            Style::default().fg(Color::Red),
        )]),
    ];

    // Add hint about what will be submitted
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Edit the code below. It will be posted as a GitHub suggestion.",
        Style::default().fg(Color::DarkGray),
    )]));

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Suggestion"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Render TextArea for suggestion input
fn render_suggestion_input_area(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let submit_key = app.input_text_area.submit_key_display();
    let title = format!("Suggested code ({}: submit, Esc: cancel)", submit_key);
    app.input_text_area
        .render_with_title(frame, area, &title, "Edit the code...");
}

/// Render context info for reply input
fn render_reply_context(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    reply_to_user: &str,
    reply_to_body: &str,
) {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Reply to ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("@{}", reply_to_user),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
    ];
    for line in reply_to_body.lines() {
        lines.push(Line::from(Span::styled(
            format!("> {}", line),
            Style::default().fg(Color::DarkGray),
        )));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Original Comment"),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// Render TextArea for reply input
fn render_reply_input_area(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let submit_key = app.input_text_area.submit_key_display();
    let title = format!("Reply ({}: submit, Esc: cancel)", submit_key);
    app.input_text_area
        .render_with_title(frame, area, &title, "Type your reply here...");
}
