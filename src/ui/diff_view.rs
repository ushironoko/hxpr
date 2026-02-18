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

/// Build a plain DiffCache without syntax highlighting (diff coloring only).
///
/// This is a fast path (~1ms) used to provide immediate visual feedback while
/// the full syntax-highlighted cache is being built in the background.
///
/// # Arguments
/// * `patch` - The diff patch content
///
/// Returns a DiffCache with file_index set to 0 (caller should update).
pub fn build_plain_diff_cache(patch: &str) -> DiffCache {
    let mut interner = Rodeo::default();
    let lines: Vec<CachedDiffLine> = patch
        .lines()
        .map(|line| {
            let (line_type, content) = classify_line(line);

            let spans = match line_type {
                LineType::Header => vec![InternedSpan {
                    content: interner.get_or_intern(line),
                    style: Style::default().fg(Color::Cyan),
                }],
                LineType::Meta => vec![InternedSpan {
                    content: interner.get_or_intern(line),
                    style: Style::default().fg(Color::Yellow),
                }],
                LineType::Added => vec![
                    InternedSpan {
                        content: interner.get_or_intern("+"),
                        style: Style::default().fg(Color::Green),
                    },
                    InternedSpan {
                        content: interner.get_or_intern(content),
                        style: Style::default().fg(Color::Green),
                    },
                ],
                LineType::Removed => vec![
                    InternedSpan {
                        content: interner.get_or_intern("-"),
                        style: Style::default().fg(Color::Red),
                    },
                    InternedSpan {
                        content: interner.get_or_intern(content),
                        style: Style::default().fg(Color::Red),
                    },
                ],
                LineType::Context => vec![
                    InternedSpan {
                        content: interner.get_or_intern(" "),
                        style: Style::default(),
                    },
                    InternedSpan {
                        content: interner.get_or_intern(content),
                        style: Style::default(),
                    },
                ],
            };

            CachedDiffLine { spans }
        })
        .collect();

    DiffCache {
        file_index: 0,
        patch_hash: hash_string(patch),
        lines,
        interner,
        highlighted: false,
    }
}

/// Build DiffCache with syntax highlighting and string interning.
///
/// Uses tree-sitter for supported languages (Rust, TypeScript, JavaScript, Go, Python)
/// and falls back to syntect for other languages.
///
/// # Arguments
/// * `patch` - The diff patch content
/// * `filename` - The filename for syntax detection
/// * `theme_name` - The theme name for syntect fallback
/// * `parser_pool` - Shared parser pool for tree-sitter parser reuse
///
/// Returns a complete DiffCache with file_index set to 0 (caller should update).
pub fn build_diff_cache(
    patch: &str,
    filename: &str,
    theme_name: &str,
    parser_pool: &mut ParserPool,
) -> DiffCache {
    let mut interner = Rodeo::default();

    // Get file extension for injection support check
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Try to build a combined source for CST highlighting
    // For CST, we need to parse the entire content at once
    // Only includes post-change lines (added + context) to ensure valid syntax
    // For SFC languages (Vue/Svelte), we may need to add priming tags
    let (combined_source, line_mapping, priming_lines) =
        build_combined_source_for_highlight_with_priming(patch, ext);

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
        // Use injection-aware highlighting for SFC languages (Svelte, Vue)
        let line_highlights = if ext == "svelte" || ext == "vue" {
            // Injection path: query is obtained inside the function to avoid borrow conflicts
            // Note: priming_lines offset is handled when applying highlights to diff lines
            collect_line_highlights_with_injections(
                &combined_source,
                &result.tree,
                result.lang,
                style_cache,
                parser_pool,
                ext,
            )
        } else {
            // Standard CST highlighting: get cached query
            let query = parser_pool
                .get_or_create_query(result.lang)
                .expect("Query should be available for supported language");
            let capture_names: Vec<String> = query
                .capture_names()
                .iter()
                .map(|s| s.to_string())
                .collect();
            collect_line_highlights(
                &combined_source,
                &result.tree,
                query,
                &capture_names,
                style_cache,
            )
        };
        build_lines_with_cst(
            patch,
            filename,
            theme_name,
            &line_highlights,
            &line_mapping,
            priming_lines,
            &mut interner,
        )
    } else {
        // Syntect fallback path (no CST support for this file type)
        build_lines_with_syntect(patch, filename, theme_name, &mut interner)
    };

    DiffCache {
        file_index: 0, // Caller should update this
        patch_hash: hash_string(patch),
        lines,
        interner,
        highlighted: true,
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

/// Check if the content looks like JavaScript/TypeScript script code.
///
/// Returns true if content contains script code patterns.
///
/// Priming is applied whenever script patterns are detected, even if template
/// or style patterns are also present (mixed content diffs). HTML `<script>`
/// elements use raw_text, so mixed content is handled correctly by the Vue
/// parser, and the TypeScript parser does error recovery on non-script parts.
///
/// Only returns false when NO script patterns are found (pure template/style
/// content should not be wrapped in `<script>` tags).
fn looks_like_script_content(source: &str) -> bool {
    // Patterns that strongly suggest script content
    let script_patterns = [
        // ES module syntax
        "import ",
        "export ",
        "from '",
        "from \"",
        // Variable declarations
        "const ",
        "let ",
        "var ",
        // Function declarations
        "function ",
        "=> {",
        "=> (",
        // Class syntax
        "class ",
        "extends ",
        // Control flow (with space to avoid matching CSS/template)
        "if (",
        "else {",
        "for (",
        "while (",
        "switch (",
        // Common JS patterns
        "return ",
        "async ",
        "await ",
        // TypeScript-specific
        "interface ",
        "type ",
        ": string",
        ": number",
        ": boolean",
        "implements ",
        "declare ",
        // Vue 3 Composition API
        "defineProps",
        "defineEmits",
        "defineExpose",
        "defineSlots",
        "ref(",
        "reactive(",
        "computed(",
        "watch(",
        "onMounted(",
        "defineComponent",
    ];

    for pattern in script_patterns {
        if source.contains(pattern) {
            return true;
        }
    }

    false
}

/// Build combined source with priming for SFC languages (Vue/Svelte).
///
/// When a diff doesn't contain SFC structural tags like `<script>`, the tree-sitter
/// parser cannot detect language injections. This function adds virtual priming tags
/// to enable proper syntax highlighting.
///
/// Returns: (source, line_mapping, priming_lines_count)
fn build_combined_source_for_highlight_with_priming(
    patch: &str,
    ext: &str,
) -> (String, Vec<(usize, LineType)>, usize) {
    let (base_source, line_mapping) = build_combined_source_for_highlight(patch);

    // Only add priming for SFC languages
    if ext != "vue" && ext != "svelte" {
        return (base_source, line_mapping, 0);
    }

    // If an opening <script> tag is present, Vue/Svelte parser can detect script injection.
    // No priming needed in this case.
    //
    // NOTE: We intentionally do NOT skip priming when only <template>/<style> is present.
    // Diff hunks may include template/style tags while omitting the opening <script> tag
    // (e.g., hidden by hunk context), which would otherwise break script injection.
    if base_source.contains("<script") {
        return (base_source, line_mapping, 0);
    }

    // No structural tags found - only prime if we're confident it's script content.
    // Being conservative: if we can't determine the content type, don't prime.
    // Wrong priming (e.g., treating template/style as script) is worse than no priming.
    if !looks_like_script_content(&base_source) {
        return (base_source, line_mapping, 0);
    }

    // Content looks like script - add priming
    // Use TypeScript as it's a superset of JavaScript
    let priming_prefix = "<script lang=\"ts\">\n";
    let priming_suffix = "</script>\n";
    let priming_lines = 1; // One line for the opening tag

    let primed_source = format!("{}{}{}", priming_prefix, base_source, priming_suffix);

    (primed_source, line_mapping, priming_lines)
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
/// * `line_highlights` - Pre-computed highlights from tree-sitter (indexed by source line)
/// * `line_mapping` - Mapping from source line index to (diff line index, line type)
/// * `priming_lines` - Number of priming lines added for SFC languages (to offset indices)
/// * `interner` - String interner for deduplication
#[allow(clippy::too_many_arguments)]
fn build_lines_with_cst(
    patch: &str,
    filename: &str,
    theme_name: &str,
    line_highlights: &crate::syntax::LineHighlights,
    line_mapping: &[(usize, LineType)],
    priming_lines: usize,
    interner: &mut Rodeo,
) -> Vec<CachedDiffLine> {
    // Build a reverse mapping: diff_line_index -> source_line_index
    // Only Added and Context lines are in the source (Removed lines are excluded)
    // Note: source_idx needs to account for priming_lines offset when looking up highlights
    let mut diff_to_source: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    for (source_idx, (diff_idx, _)) in line_mapping.iter().enumerate() {
        // Add priming_lines to map to the actual line in the primed source
        diff_to_source.insert(*diff_idx, source_idx + priming_lines);
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

            CachedDiffLine { spans }
        })
        .collect()
}

/// Build cached lines using syntect highlighting (fallback).
fn build_lines_with_syntect(
    patch: &str,
    filename: &str,
    theme_name: &str,
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
        .map(|line| {
            let (line_type, content) = classify_line(line);

            let spans = build_line_spans(line_type, line, content, &mut highlighter, interner);

            CachedDiffLine { spans }
        })
        .collect()
}

/// Convert cached diff lines to renderable [`Line`]s using zero-copy borrowing.
///
/// Resolves interned strings from the DiffCache's interner, avoiding heap
/// allocations entirely. Comment markers (`● `) are injected at render time
/// via iterator composition (no `Vec::insert`).
///
/// * `cache` – the DiffCache containing both lines and the interner.
/// * `range` – the range of lines to render (may be a sub-range).
/// * `selected_line` – absolute index of the currently selected line.
/// * `comment_lines` – set of diff line indices that have comments (for `●` marker).
pub fn render_cached_lines<'a>(
    cache: &'a DiffCache,
    range: std::ops::Range<usize>,
    selected_line: usize,
    comment_lines: &HashSet<usize>,
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

            let marker = if comment_lines.contains(&abs_idx) {
                Some(Span::styled("● ", Style::default().fg(Color::Yellow)))
            } else {
                None
            };
            let base = cached
                .spans
                .iter()
                .map(|s| Span::styled(cache.resolve(s.content), s.style));
            let all_spans: Vec<Span<'_>> = marker.into_iter().chain(base).collect();

            if is_selected {
                Line::from(all_spans).style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                Line::from(all_spans)
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
        render_cached_lines(
            cache,
            visible_start..visible_end,
            app.selected_line,
            &app.file_comment_lines,
        )
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
    let cache = build_diff_cache(patch, filename, theme_name, &mut parser_pool);

    cache
        .lines
        .iter()
        .enumerate()
        .map(|(i, cached)| {
            let is_selected = i == selected_line;
            let marker = if comment_lines.contains(&i) {
                Some(Span::styled(
                    "● ".to_string(),
                    if is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::Yellow)
                    },
                ))
            } else {
                None
            };
            let base = cached.spans.iter().map(|s| {
                let text = cache.resolve(s.content).to_string();
                let mut style = s.style;
                if is_selected {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                Span::styled(text, style)
            });
            let all_spans: Vec<Span<'static>> = marker.into_iter().chain(base).collect();
            Line::from(all_spans)
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

    let footer_line = super::footer::build_footer_line(app, help_text);
    let footer = Paragraph::new(footer_line).block(Block::default().borders(Borders::ALL));
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
        let cache = build_diff_cache(patch, "test.rs", "Dracula", &mut parser_pool);

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
        let cache = build_diff_cache(patch, "test.rs", "Dracula", &mut parser_pool);

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
        let cache = build_diff_cache(patch, "test.ts", "Dracula", &mut parser_pool);

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

    #[test]
    fn test_vue_priming_for_script_only_diff() {
        use ratatui::style::Color;

        // Vue diff that only contains script content (no <script> tag)
        // This simulates editing inside a script block
        let patch = r#"@@ -5,3 +5,4 @@
 const count = ref(0);
+const doubled = computed(() => count.value * 2);
 function increment() {"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(patch, "Component.vue", "Dracula", &mut parser_pool);

        // Line 2 is "+const doubled = computed(() => count.value * 2);" (Added line)
        let added_line = &cache.lines[2];

        // Find the "const" keyword - should be syntax highlighted via priming
        let const_span = added_line
            .spans
            .iter()
            .find(|s| cache.resolve(s.content).contains("const"));
        assert!(
            const_span.is_some(),
            "Vue script content should have 'const' highlighted via priming. Spans: {:?}",
            added_line
                .spans
                .iter()
                .map(|s| cache.resolve(s.content))
                .collect::<Vec<_>>()
        );

        let const_style = const_span.unwrap().style;

        // "const" should be syntax highlighted, not plain green (added line default)
        match const_style.fg {
            Some(Color::Green) => {
                panic!(
                    "'const' in Vue diff has plain green color (added line default). \
                     Priming should enable TypeScript syntax highlighting."
                );
            }
            Some(Color::Rgb(r, g, b)) => {
                // Should be some syntax color, not plain green (0, 128, 0)
                assert!(
                    !(r == 0 && g == 128 && b == 0),
                    "'const' should have syntax highlighting. Got Rgb({}, {}, {})",
                    r,
                    g,
                    b
                );
            }
            None => {
                panic!("'const' in Vue script should have a foreground color");
            }
            _ => {}
        }
    }

    #[test]
    fn test_vue_no_priming_when_script_tag_present() {
        // Vue diff that already has <script> tag - no priming needed
        let patch = r#"@@ -1,5 +1,6 @@
 <script lang="ts">
 const count = ref(0);
+const doubled = computed(() => count.value * 2);
 </script>"#;

        let (source, line_mapping, priming_lines) =
            build_combined_source_for_highlight_with_priming(patch, "vue");

        assert_eq!(
            priming_lines, 0,
            "Should not add priming when <script> tag is present"
        );
        assert!(
            source.contains("<script"),
            "Source should contain original <script> tag"
        );
        assert_eq!(line_mapping.len(), 4, "Should have 4 mapped lines");
    }

    #[test]
    fn test_vue_priming_adds_script_wrapper() {
        // Vue diff without any SFC tags - needs priming
        let patch = r#"@@ -5,2 +5,3 @@
 const count = ref(0);
+const doubled = computed(() => count.value * 2);"#;

        let (source, line_mapping, priming_lines) =
            build_combined_source_for_highlight_with_priming(patch, "vue");

        assert_eq!(priming_lines, 1, "Should add 1 priming line for <script>");
        assert!(
            source.starts_with("<script lang=\"ts\">\n"),
            "Source should start with priming <script> tag"
        );
        assert!(
            source.ends_with("</script>\n"),
            "Source should end with closing </script> tag"
        );
        assert_eq!(line_mapping.len(), 2, "Line mapping should be unchanged");
    }

    #[test]
    fn test_vue_priming_when_template_tag_present_but_script_tag_missing() {
        // Vue diff where template hunk is visible, but opening <script> tag is hidden by hunk context.
        // We should still prime script wrapper so script lines get injection highlighting.
        let patch = r#"@@ -7,4 +7,4 @@
 import { ref } from 'vue'
 const count = ref(0)
-const oldValue = computed(() => count.value)
+const newValue = computed(() => count.value)
@@ -40,5 +40,6 @@
 <template>
   <div class="foo">
+    <span>{{ newValue }}</span>
   </div>
 </template>"#;

        let (source, line_mapping, priming_lines) =
            build_combined_source_for_highlight_with_priming(patch, "vue");

        assert_eq!(
            priming_lines, 1,
            "Should add priming when <script> start tag is missing, even if <template> exists"
        );
        assert!(
            source.starts_with("<script lang=\"ts\">\n"),
            "Source should start with priming <script> tag"
        );
        assert!(
            source.contains("<template>"),
            "Original template content should still be present"
        );
        assert_eq!(
            line_mapping.len(),
            8,
            "Line mapping should preserve source lines"
        );
    }

    #[test]
    fn test_non_sfc_no_priming() {
        // TypeScript file - no priming needed
        let patch = r#"@@ -1,2 +1,3 @@
 const x = 1;
+const y = 2;"#;

        let (source, _, priming_lines) =
            build_combined_source_for_highlight_with_priming(patch, "ts");

        assert_eq!(priming_lines, 0, "TypeScript should not have priming");
        assert!(
            !source.contains("<script"),
            "TypeScript source should not have <script> tag"
        );
    }

    #[test]
    fn test_build_combined_source_basic() {
        let patch = r#"@@ -1,3 +1,3 @@
 context line
-removed line
+added line"#;

        let (source, mapping) = build_combined_source_for_highlight(patch);

        // Removed 行が除外されていること
        assert!(!source.contains("removed line"));
        // Added/Context 行のみ含まれること
        assert!(source.contains("context line"));
        assert!(source.contains("added line"));

        // マッピングサイズ: context(1) + added(2) = 2行（header/removed は除外）
        assert_eq!(mapping.len(), 2);
        // マッピング: (diff_line_idx, line_type)
        assert_eq!(mapping[0], (1, LineType::Context)); // " context line"
        assert_eq!(mapping[1], (3, LineType::Added)); // "+added line"
    }

    #[test]
    fn test_build_combined_source_multiple_hunks() {
        let patch = r#"@@ -1,2 +1,2 @@
 first
-old
+new
@@ -10,2 +10,2 @@
 second
+another"#;

        let (source, mapping) = build_combined_source_for_highlight(patch);

        // 4行: first, new, second, another
        assert_eq!(mapping.len(), 4);

        // diff_line_idx がジャンプすること（ハンクヘッダ分）
        assert_eq!(mapping[0].0, 1); // " first"
        assert_eq!(mapping[1].0, 3); // "+new"
        assert_eq!(mapping[2].0, 5); // " second"
        assert_eq!(mapping[3].0, 6); // "+another"

        // ソースにヘッダが含まれないこと
        assert!(!source.contains("@@"));
    }

    #[test]
    fn test_build_combined_source_empty_patch() {
        let (source, mapping) = build_combined_source_for_highlight("");
        assert!(source.is_empty());
        assert!(mapping.is_empty());
    }

    #[test]
    fn plain_and_highlighted_cache_have_same_line_count() {
        let patch = "diff --git a/foo.rs b/foo.rs\n--- a/foo.rs\n+++ b/foo.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    println!(\"hello\");\n+    println!(\"world\");\n }";
        let mut parser_pool = ParserPool::new();

        let plain = build_plain_diff_cache(patch);
        let highlighted = build_diff_cache(patch, "foo.rs", "base16-ocean.dark", &mut parser_pool);

        assert_eq!(plain.lines.len(), highlighted.lines.len());

        // highlighted フラグの検証
        assert!(!plain.highlighted, "plain cache should not be highlighted");
        assert!(
            highlighted.highlighted,
            "highlighted cache should be highlighted"
        );
    }

    #[test]
    fn render_cached_lines_inserts_comment_markers() {
        let patch = "diff --git a/foo.rs b/foo.rs\n--- a/foo.rs\n+++ b/foo.rs\n@@ -1,3 +1,3 @@\n fn main() {\n-    println!(\"hello\");\n+    println!(\"world\");\n }";
        // コメントマーカーがレンダリング時に挿入されることを検証
        let mut comment_lines = HashSet::new();
        comment_lines.insert(4); // 行インデックス4（context行）
        comment_lines.insert(6); // 行インデックス6（added行）
        let mut parser_pool = ParserPool::new();

        let plain = build_plain_diff_cache(patch);
        let highlighted = build_diff_cache(patch, "foo.rs", "base16-ocean.dark", &mut parser_pool);

        assert_eq!(plain.lines.len(), highlighted.lines.len());

        // キャッシュ自体にはコメントマーカーが含まれないことを確認
        let plain_first = plain.resolve(plain.lines[4].spans[0].content);
        assert!(
            !plain_first.contains('●'),
            "plain cache should not contain comment marker in spans"
        );

        // render_cached_lines でコメントマーカーが挿入されること
        let plain_rendered = render_cached_lines(&plain, 0..plain.lines.len(), 0, &comment_lines);
        let hl_rendered =
            render_cached_lines(&highlighted, 0..highlighted.lines.len(), 0, &comment_lines);

        for &line_idx in &[4usize, 6] {
            let plain_line_text: String = plain_rendered[line_idx]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect();
            let hl_line_text: String = hl_rendered[line_idx]
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect();
            assert!(
                plain_line_text.contains('●'),
                "plain rendered line {} should have comment marker, got: {:?}",
                line_idx,
                plain_line_text,
            );
            assert!(
                hl_line_text.contains('●'),
                "highlighted rendered line {} should have comment marker, got: {:?}",
                line_idx,
                hl_line_text,
            );
        }

        // コメントのない行にはマーカーがないこと
        let no_comment_text: String = plain_rendered[0]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            !no_comment_text.contains('●'),
            "non-comment line should not have marker"
        );
    }

    #[test]
    fn test_build_plain_diff_cache_line_styles() {
        // 全 LineType を含むパッチ
        let patch = "diff --git a/foo.rs b/foo.rs\n@@ -1,3 +1,3 @@\n context\n+added\n-removed";
        let cache = build_plain_diff_cache(patch);

        assert_eq!(cache.lines.len(), 5);

        // Meta 行 (diff --git): Yellow, 単一スパン
        let meta = &cache.lines[0];
        assert_eq!(meta.spans.len(), 1);
        assert_eq!(meta.spans[0].style.fg, Some(Color::Yellow));

        // Header 行 (@@): Cyan, 単一スパン
        let header = &cache.lines[1];
        assert_eq!(header.spans.len(), 1);
        assert_eq!(header.spans[0].style.fg, Some(Color::Cyan));

        // Context 行: " " マーカー + コンテンツ, default style
        let context = &cache.lines[2];
        assert_eq!(context.spans.len(), 2);
        assert_eq!(cache.resolve(context.spans[0].content), " ");
        assert_eq!(context.spans[0].style.fg, None);

        // Added 行: "+" マーカー + コンテンツ, Green
        let added = &cache.lines[3];
        assert_eq!(added.spans.len(), 2);
        assert_eq!(cache.resolve(added.spans[0].content), "+");
        assert_eq!(added.spans[0].style.fg, Some(Color::Green));
        assert_eq!(added.spans[1].style.fg, Some(Color::Green));

        // Removed 行: "-" マーカー + コンテンツ, Red
        let removed = &cache.lines[4];
        assert_eq!(removed.spans.len(), 2);
        assert_eq!(cache.resolve(removed.spans[0].content), "-");
        assert_eq!(removed.spans[0].style.fg, Some(Color::Red));
        assert_eq!(removed.spans[1].style.fg, Some(Color::Red));
    }

    #[test]
    fn test_parse_patch_to_lines_basic() {
        let patch = r#"@@ -1,3 +1,3 @@
 context line
-removed line
+added line"#;

        let comment_lines = HashSet::new();
        let lines = parse_patch_to_lines(patch, 0, "test.rs", "base16-ocean.dark", &comment_lines);

        // 4行: header, context, removed, added
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_parse_patch_to_lines_with_comments_and_selection() {
        let patch = r#"@@ -1,2 +1,3 @@
 context
+added
-removed"#;

        let mut comment_lines = HashSet::new();
        comment_lines.insert(2); // added 行にコメント

        let lines = parse_patch_to_lines(patch, 2, "test.rs", "base16-ocean.dark", &comment_lines);

        assert_eq!(lines.len(), 4);

        // selected_line=2 の行は REVERSED modifier を持つ
        let selected_line = &lines[2];
        let has_reversed = selected_line
            .spans
            .iter()
            .any(|s| s.style.add_modifier.contains(Modifier::REVERSED));
        assert!(has_reversed, "Selected line should have REVERSED modifier");

        // コメントマーカー（● ）が挿入されていること
        let comment_line_text: String = selected_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            comment_line_text.contains('●'),
            "Comment line should have ● marker, got: {:?}",
            comment_line_text
        );
    }

    #[test]
    fn test_render_cached_lines_out_of_bounds_range() {
        let patch = "@@ -1,2 +1,2 @@\n context\n+added\n-removed";
        let cache = build_plain_diff_cache(patch);
        assert_eq!(cache.lines.len(), 4);

        // range が完全に範囲外 → 空の Vec
        let result = render_cached_lines(&cache, 100..200, 0, &HashSet::new());
        assert!(
            result.is_empty(),
            "Out-of-bounds range should return empty Vec"
        );
    }

    #[test]
    fn test_render_cached_lines_empty_cache() {
        let cache = build_plain_diff_cache("");
        assert!(cache.lines.is_empty());

        let result = render_cached_lines(&cache, 0..10, 0, &HashSet::new());
        assert!(result.is_empty(), "Empty cache should return empty Vec");
    }
}

#[cfg(test)]
mod priming_diff_tests {
    use super::*;
    use crate::syntax::ParserPool;

    #[test]
    fn test_build_diff_cache_primed_vue() {
        // Simulate a diff that contains only script content (no <script> tag)
        let patch = r#"diff --git a/src/composables/useFoo.ts b/src/composables/useFoo.ts
@@ -1,5 +1,7 @@
+import { ref } from 'vue'
+
 export const useFoo = () => {
-  const old = 1
+  const count = ref(0)
   return { count }
 }
"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(
            patch,
            "src/components/Foo.vue",
            "base16-ocean.dark",
            &mut parser_pool,
        );

        // The import line should have syntax highlighting
        // Line 2 is "+import { ref } from 'vue'"
        let import_line = &cache.lines[2];
        assert!(
            import_line.spans.len() > 2,
            "Import line should have syntax highlighting (more than just marker), got {} spans",
            import_line.spans.len()
        );
    }

    #[test]
    fn test_build_diff_cache_primed_vue_mixed_content() {
        // Simulate a diff with BOTH script and template content (no structural tags).
        // This is the common case when a Vue SFC diff spans multiple hunks across
        // script and template sections.
        let patch = r#"diff --git a/src/components/Foo.vue b/src/components/Foo.vue
@@ -7,14 +7,13 @@
+import { ref } from 'vue'
 import SomeComponent from '@/components/SomeComponent.vue'
-import OldComponent from '@/components/OldComponent.vue'

 const count = ref(0)
@@ -80,5 +79,5 @@
     </div>
-    <OldDialog />
+    <NewDialog @close="closeDialog" />
   </div>
"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(
            patch,
            "src/components/Foo.vue",
            "base16-ocean.dark",
            &mut parser_pool,
        );

        // Find import and const lines by content
        let mut import_idx = None;
        let mut const_idx = None;
        for (i, line) in cache.lines.iter().enumerate() {
            let text: String = line
                .spans
                .iter()
                .map(|s| cache.resolve(s.content).to_string())
                .collect();
            if text.contains("import { ref }") {
                import_idx = Some(i);
            }
            if text.contains("const count") {
                const_idx = Some(i);
            }
        }

        // The import line should have TypeScript highlighting
        let import_line = &cache.lines[import_idx.expect("import line not found")];
        assert!(
            import_line.spans.len() > 2,
            "Import line in mixed content should have syntax highlighting, got {} spans",
            import_line.spans.len()
        );

        // The const line should also have TypeScript highlighting
        let const_line = &cache.lines[const_idx.expect("const line not found")];
        assert!(
            const_line.spans.len() > 2,
            "Const line in mixed content should have syntax highlighting, got {} spans",
            const_line.spans.len()
        );
    }

    #[test]
    fn test_build_diff_cache_primed_vue_with_visible_template_but_hidden_script_tag() {
        // Simulate a diff where the template hunk includes <template>, but <script> start
        // tag is outside hunk context. Script lines should still be highlighted.
        let patch = r#"diff --git a/src/components/Foo.vue b/src/components/Foo.vue
@@ -7,4 +7,4 @@
 import { ref } from 'vue'
 const count = ref(0)
-const oldValue = computed(() => count.value)
+const newValue = computed(() => count.value)
@@ -40,5 +40,6 @@
 <template>
   <div class="foo">
+    <span>{{ newValue }}</span>
   </div>
 </template>
"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(
            patch,
            "src/components/Foo.vue",
            "base16-ocean.dark",
            &mut parser_pool,
        );

        // Find the updated const line by content and ensure tokenized highlighting exists.
        let const_line = cache
            .lines
            .iter()
            .find(|line| {
                let text: String = line
                    .spans
                    .iter()
                    .map(|s| cache.resolve(s.content).to_string())
                    .collect();
                text.contains("const newValue = computed")
            })
            .expect("const line not found");

        assert!(
            const_line.spans.len() > 2,
            "Script line should have syntax highlighting even when <template> is visible"
        );
    }

    #[test]
    fn test_build_diff_cache_syntect_fallback() {
        // CST 非対応拡張子 (.yaml) → syntect フォールバックパス
        let patch = r#"@@ -1,3 +1,4 @@
 name: test
+version: "1.0"
 description: hello
 author: world"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(patch, "data.yaml", "base16-ocean.dark", &mut parser_pool);

        // 行数が正しいこと（header + 4 content lines = 5）
        assert_eq!(cache.lines.len(), 5);
        assert!(
            cache.highlighted,
            "syntect path should set highlighted=true"
        );

        // Added 行の先頭スパンが "+" マーカー（Green）
        let added_line = &cache.lines[2]; // "+version: \"1.0\""
        assert_eq!(cache.resolve(added_line.spans[0].content), "+");
        assert_eq!(
            added_line.spans[0].style.fg,
            Some(Color::Green),
            "Added line marker should be Green"
        );
    }

    #[test]
    fn test_build_diff_cache_no_syntax_support() {
        // シンタックスサポートなし (.unknown) → highlight_or_fallback(None) パス
        let patch = r#"@@ -1,2 +1,3 @@
 existing line
+new line
-old line"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(patch, "file.unknown", "base16-ocean.dark", &mut parser_pool);

        assert_eq!(cache.lines.len(), 4);

        // Added 行: フォールバックカラー Green
        let added_line = &cache.lines[2];
        assert_eq!(cache.resolve(added_line.spans[0].content), "+");
        // コンテンツスパンもフォールバック色が適用
        let added_content_style = added_line.spans[1].style;
        assert_eq!(
            added_content_style.fg,
            Some(Color::Green),
            "No-syntax added content should use Green fallback"
        );

        // Removed 行: フォールバックカラー Red
        let removed_line = &cache.lines[3];
        assert_eq!(cache.resolve(removed_line.spans[0].content), "-");
        let removed_content_style = removed_line.spans[1].style;
        assert_eq!(
            removed_content_style.fg,
            Some(Color::Red),
            "No-syntax removed content should use Red fallback"
        );
    }

    #[test]
    fn test_build_lines_with_syntect_vue_priming() {
        // build_lines_with_syntect を直接呼出し、Vue プライミングパス (L470-476) をカバー
        let patch = r#"@@ -1,2 +1,3 @@
 const x = 1;
+const y = 2;"#;

        let mut interner = Rodeo::default();
        let lines =
            build_lines_with_syntect(patch, "Component.vue", "base16-ocean.dark", &mut interner);

        assert_eq!(lines.len(), 3);

        // Header 行
        let header_text = interner.resolve(&lines[0].spans[0].content);
        assert!(header_text.starts_with("@@"));

        // Context 行のマーカー
        let context_marker = interner.resolve(&lines[1].spans[0].content);
        assert_eq!(context_marker, " ");

        // Added 行のマーカー
        let added_marker = interner.resolve(&lines[2].spans[0].content);
        assert_eq!(added_marker, "+");
    }

    #[test]
    fn test_looks_like_script_content_mixed() {
        // Mixed content should still be detected as script
        let source = "import { ref } from 'vue'\nconst count = ref(0)\n</div>\n<NewDialog @close=\"closeDialog\" />\n";
        assert!(
            looks_like_script_content(source),
            "Mixed script+template content should be detected as script"
        );
    }

    #[test]
    fn test_looks_like_script_content_pure_template() {
        // Pure template content should NOT be detected as script
        let source = "<div>\n  <span>hello</span>\n</div>\n";
        assert!(
            !looks_like_script_content(source),
            "Pure template content should not be detected as script"
        );
    }
}
