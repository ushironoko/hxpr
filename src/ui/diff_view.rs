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
    apply_line_highlights, collect_line_highlights, collect_line_highlights_with_injections,
    get_theme, highlight_code_line, syntax_for_file, Highlighter, ParserPool,
};

/// Build DiffCache with syntax highlighting and string interning.
///
/// Uses tree-sitter for supported languages (Rust, TypeScript, JavaScript, Go, Python)
/// and falls back to syntect for other languages.
///
/// # Arguments
/// * `patch` - The diff patch content
/// * `filename` - The filename for syntax detection
/// * `theme_name` - The theme name for syntect fallback
/// * `comment_lines` - Set of line indices that have comments
/// * `parser_pool` - Shared parser pool for tree-sitter parser reuse
///
/// Returns a complete DiffCache with file_index set to 0 (caller should update).
pub fn build_diff_cache(
    patch: &str,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
    parser_pool: &mut ParserPool,
) -> DiffCache {
    let mut interner = Rodeo::default();

    // Try to build a combined source for CST highlighting
    // For CST, we need to parse the entire content at once
    // Only includes post-change lines (added + context) to ensure valid syntax
    let (combined_source, line_mapping) = build_combined_source_for_highlight(patch);

    // Get file extension for injection support check
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Create highlighter (does not borrow parser_pool)
    let highlighter = Highlighter::for_file(filename, theme_name);

    // Parse source (borrows parser_pool only for this call)
    let cst_result = highlighter.parse_source(&combined_source, parser_pool);

    // Check if tree-sitter parsing succeeded
    // Note: We no longer fall back based on error count, since tree-sitter's
    // error recovery produces usable AST even with parse errors. The errors
    // typically occur because diffs contain incomplete code (missing context
    // between hunks), not because the code is actually invalid.
    let use_cst = cst_result.is_some();

    let lines: Vec<CachedDiffLine> = if use_cst {
        let result = cst_result.as_ref().unwrap();
        let style_cache = highlighter
            .style_cache()
            .expect("CST highlighter should have style_cache");

        // CST path: use tree-sitter with full AST context
        // Use injection-aware highlighting for SFC languages (Svelte)
        let line_highlights = if ext == "svelte" {
            collect_line_highlights_with_injections(
                &combined_source,
                &result.tree,
                &result.query,
                &result.capture_names,
                style_cache,
                parser_pool,
                ext,
            )
        } else {
            // Standard CST highlighting for other languages
            collect_line_highlights(
                &combined_source,
                &result.tree,
                &result.query,
                &result.capture_names,
                style_cache,
            )
        };
        build_lines_with_cst(
            patch,
            filename,
            theme_name,
            comment_lines,
            &line_highlights,
            &line_mapping,
            &mut interner,
        )
    } else {
        // Syntect fallback path (no CST support for this file type)
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
///
/// **IMPORTANT**: Only includes added lines and context lines (post-change version).
/// Removed lines are excluded to avoid creating syntactically invalid code,
/// especially for indentation-sensitive languages like Python.
///
/// Returns both the source string and a mapping from source line index to diff line info
/// for accurate highlight application.
fn build_combined_source_for_highlight(patch: &str) -> (String, Vec<(usize, LineType)>) {
    let mut source = String::new();
    // Maps source line index -> (diff line index, line type)
    let mut line_mapping: Vec<(usize, LineType)> = Vec::new();

    for (diff_line_idx, line) in patch.lines().enumerate() {
        let (line_type, content) = classify_line(line);
        match line_type {
            // Only include added and context lines (post-change version)
            // This ensures the source is syntactically valid for tree-sitter
            LineType::Added | LineType::Context => {
                line_mapping.push((diff_line_idx, line_type));
                source.push_str(content);
                source.push('\n');
            }
            // Skip removed lines to maintain valid syntax (especially for Python)
            // Skip headers and meta lines
            LineType::Removed | LineType::Header | LineType::Meta => {}
        }
    }
    (source, line_mapping)
}

/// Build cached lines using CST highlighting.
///
/// Uses pre-computed line highlights to avoid per-line tree traversal.
/// Removed lines (which are excluded from CST source for valid syntax) are
/// highlighted using syntect as a fallback to maintain syntax coloring.
///
/// # Arguments
/// * `patch` - The original diff patch
/// * `filename` - The filename for syntect fallback highlighting
/// * `theme_name` - The theme name for syntect fallback highlighting
/// * `comment_lines` - Set of line indices with comments
/// * `line_highlights` - Pre-computed highlights from tree-sitter (indexed by source line)
/// * `line_mapping` - Mapping from source line index to (diff line index, line type)
/// * `interner` - String interner for deduplication
fn build_lines_with_cst(
    patch: &str,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
    line_highlights: &crate::syntax::LineHighlights,
    line_mapping: &[(usize, LineType)],
    interner: &mut Rodeo,
) -> Vec<CachedDiffLine> {
    // Build a reverse mapping: diff_line_index -> source_line_index
    // Only Added and Context lines are in the source (Removed lines are excluded)
    let mut diff_to_source: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for (source_idx, (diff_idx, _)) in line_mapping.iter().enumerate() {
        diff_to_source.insert(*diff_idx, source_idx);
    }

    // Create syntect highlighter for removed lines (they're not in CST source)
    // This is needed because removed lines are excluded from CST to maintain valid syntax
    let syntax = syntax_for_file(filename);
    let theme = get_theme(theme_name);
    let mut syntect_highlighter = syntax.map(|s| HighlightLines::new(s, theme));

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
                LineType::Added | LineType::Context => {
                    // These lines are in the CST source, look up their highlights
                    let source_line_index = diff_to_source.get(&i).copied();

                    let marker_style = match line_type {
                        LineType::Added => Style::default().fg(Color::Green),
                        _ => Style::default(),
                    };

                    let marker = match line_type {
                        LineType::Added => "+",
                        LineType::Context => " ",
                        _ => "",
                    };

                    let mut spans = vec![InternedSpan {
                        content: interner.get_or_intern(marker),
                        style: marker_style,
                    }];

                    // Apply pre-computed highlights for this line (O(1) lookup)
                    let captures = source_line_index.and_then(|idx| line_highlights.get(idx));
                    let code_spans = apply_line_highlights(content, captures, interner);
                    spans.extend(code_spans);
                    spans
                }
                LineType::Removed => {
                    // Removed lines are NOT in the CST source (to preserve valid syntax)
                    // Use syntect to apply syntax highlighting (fallback)
                    let marker = InternedSpan {
                        content: interner.get_or_intern("-"),
                        style: Style::default().fg(Color::Red),
                    };
                    let code_spans = highlight_or_fallback(
                        content,
                        &mut syntect_highlighter,
                        Color::Red,
                        interner,
                    );
                    std::iter::once(marker).chain(code_spans).collect()
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

    // For Vue files, prime the highlighter by processing a virtual <script> tag.
    // This puts syntect into JavaScript mode so that code outside the actual
    // <script> tag (which may not be included in the diff hunk) gets highlighted.
    if filename.ends_with(".vue") {
        if let Some(ref mut hl) = highlighter {
            let ss = two_face::syntax::extra_newlines();
            // Process virtual script tag to enter JavaScript mode
            let _ = hl.highlight_line("<script lang=\"ts\">\n", &ss);
        }
    }

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
    // Clamp range to valid bounds to prevent out-of-bounds panic
    let len = cache.lines.len();
    let safe_start = range.start.min(len);
    let safe_end = range.end.min(len);
    // Handle case where start > end after clamping (produces empty slice)
    let safe_range = safe_start..safe_start.max(safe_end);

    cache.lines[safe_range.clone()]
        .iter()
        .enumerate()
        .map(|(rel_idx, cached)| {
            let abs_idx = safe_range.start + rel_idx;
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

/// Fallback function to render patch lines when cache is not available.
///
/// This function is called from `render_diff_content` when `app.diff_cache` is None,
/// which should rarely happen in normal operation since `App::ensure_diff_cache()`
/// is called before rendering.
///
/// NOTE: Creates a temporary ParserPool instead of reusing App's pool. This is acceptable
/// because this fallback path is rarely executed - the main code path uses
/// `App::ensure_diff_cache()` which properly reuses the shared parser pool.
/// Hoisting a shared pool here would require passing &mut App through the render
/// chain, which conflicts with the immutable borrow pattern in render functions.
fn parse_patch_to_lines(
    patch: &str,
    selected_line: usize,
    filename: &str,
    theme_name: &str,
    comment_lines: &HashSet<usize>,
) -> Vec<Line<'static>> {
    // Build DiffCache and then convert to Lines
    // This ensures consistent behavior with cached path
    // Creates a temporary ParserPool - this is acceptable for this rarely-used fallback path
    let mut parser_pool = ParserPool::new();
    let cache = build_diff_cache(patch, filename, theme_name, comment_lines, &mut parser_pool);

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
            render_text_input_area(
                frame,
                app,
                chunks[2],
                "Comment",
                "Type your comment here...",
            );
        }
        Some(InputMode::Suggestion {
            context,
            original_code,
        }) => {
            render_suggestion_context(frame, app, chunks[1], context, original_code);
            render_text_input_area(frame, app, chunks[2], "Suggested code", "Edit the code...");
        }
        Some(InputMode::Reply {
            reply_to_user,
            reply_to_body,
            ..
        }) => {
            render_reply_context(frame, chunks[1], reply_to_user, reply_to_body);
            render_text_input_area(frame, app, chunks[2], "Reply", "Type your reply here...");
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

/// Render TextArea with dynamic title and placeholder
fn render_text_input_area(
    frame: &mut Frame,
    app: &App,
    area: ratatui::layout::Rect,
    label: &str,
    placeholder: &str,
) {
    let submit_key = app.input_text_area.submit_key_display();
    let title = format!("{} ({}: submit, Esc: cancel)", label, submit_key);
    app.input_text_area
        .render_with_title(frame, area, &title, placeholder);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_diff_cache_with_dracula_theme() {
        use ratatui::style::Color;

        let patch = r#"@@ -1,5 +1,6 @@
 use std::collections::HashMap;

 fn main() {
+    let x = 42;
     println!("Hello");
 }"#;

        let mut parser_pool = ParserPool::new();
        let comment_lines = HashSet::new();
        let cache = build_diff_cache(
            patch,
            "test.rs",
            "Dracula",
            &comment_lines,
            &mut parser_pool,
        );

        // Line 1 is " use std::collections::HashMap;" (Context line)
        // Find the "use" keyword span
        let line1 = &cache.lines[1]; // Skip the @@ header
        let use_span = line1
            .spans
            .iter()
            .find(|s| cache.resolve(s.content) == "use");
        assert!(use_span.is_some(), "Should have 'use' span in line 1");

        let use_style = use_span.unwrap().style;

        // Dracula pink is Rgb(255, 121, 198)
        match use_style.fg {
            Some(Color::Rgb(255, 121, 198)) => {}
            Some(Color::Rgb(r, g, b)) => {
                panic!(
                    "'use' has wrong color. Expected Rgb(255, 121, 198), got Rgb({}, {}, {})",
                    r, g, b
                );
            }
            other => {
                panic!("Expected Rgb color for 'use' keyword, got {:?}", other);
            }
        }
    }

    #[test]
    fn test_removed_lines_have_syntax_highlighting_in_cst_path() {
        use ratatui::style::Color;

        // This patch includes removed lines that should be syntax highlighted
        // even when CST (tree-sitter) is used for added/context lines
        let patch = r#"@@ -1,5 +1,5 @@
 use std::collections::HashMap;

 fn main() {
-    let old_value = 100;
+    let new_value = 200;
 }"#;

        let mut parser_pool = ParserPool::new();
        let comment_lines = HashSet::new();
        let cache = build_diff_cache(
            patch,
            "test.rs",
            "Dracula",
            &comment_lines,
            &mut parser_pool,
        );

        // Line 4 is "-    let old_value = 100;" (Removed line)
        // Find the "let" keyword span - it should be syntax highlighted, not plain red
        let removed_line = &cache.lines[4];

        // First span should be the "-" marker
        assert_eq!(cache.resolve(removed_line.spans[0].content), "-");

        // Find the "let" keyword in the removed line
        let let_span = removed_line
            .spans
            .iter()
            .find(|s| cache.resolve(s.content) == "let");
        assert!(
            let_span.is_some(),
            "Removed line should have 'let' span with syntax highlighting"
        );

        let let_style = let_span.unwrap().style;

        // "let" should have syntax highlighting (Dracula cyan for keywords)
        // NOT plain red (Color::Red)
        match let_style.fg {
            Some(Color::Red) => {
                panic!(
                    "'let' in removed line has plain red color. \
                     It should have syntax highlighting (e.g., Dracula cyan)."
                );
            }
            Some(Color::Rgb(r, g, b)) => {
                // Should be some syntax color (not pure red 255,0,0)
                assert!(
                    !(r == 255 && g == 0 && b == 0),
                    "'let' should have syntax highlighting, not plain red"
                );
            }
            None => {
                panic!("'let' in removed line should have a foreground color");
            }
            _ => {
                // Other colors are acceptable (theme-dependent)
            }
        }
    }

    #[test]
    fn test_removed_lines_typescript_highlighting() {
        use ratatui::style::Color;

        let patch = r#"@@ -1,3 +1,3 @@
-const oldValue = 42;
+const newValue = 100;
 export default oldValue;"#;

        let mut parser_pool = ParserPool::new();
        let comment_lines = HashSet::new();
        let cache = build_diff_cache(
            patch,
            "test.ts",
            "Dracula",
            &comment_lines,
            &mut parser_pool,
        );

        // Line 1 is "-const oldValue = 42;" (Removed line)
        let removed_line = &cache.lines[1];

        // Find the "const" keyword in the removed line
        let const_span = removed_line
            .spans
            .iter()
            .find(|s| cache.resolve(s.content) == "const");
        assert!(
            const_span.is_some(),
            "Removed TypeScript line should have 'const' span with syntax highlighting"
        );

        let const_style = const_span.unwrap().style;

        // "const" should be syntax highlighted, not plain red
        match const_style.fg {
            Some(Color::Red) => {
                panic!(
                    "'const' in removed TypeScript line has plain red color. \
                     It should have syntax highlighting."
                );
            }
            Some(Color::Rgb(r, g, b)) => {
                // Should be some syntax color (Dracula cyan is approximately (139, 233, 253))
                assert!(
                    !(r == 255 && g == 0 && b == 0),
                    "'const' should have syntax highlighting, not plain red. Got Rgb({}, {}, {})",
                    r,
                    g,
                    b
                );
            }
            None => {
                panic!("'const' in removed line should have a foreground color");
            }
            _ => {}
        }
    }
}
