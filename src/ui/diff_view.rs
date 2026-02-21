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
        markdown_rich: false,
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
/// * `markdown_rich` - Whether to apply markdown rich display overrides
///
/// Returns a complete DiffCache with file_index set to 0 (caller should update).
pub fn build_diff_cache(
    patch: &str,
    filename: &str,
    theme_name: &str,
    parser_pool: &mut ParserPool,
    markdown_rich: bool,
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
        let base_style_cache = highlighter
            .style_cache()
            .expect("CST highlighter should have style_cache");

        // Apply markdown rich overrides when enabled for markdown files
        let rich_cache;
        let style_cache = if markdown_rich && (ext == "md" || ext == "markdown") {
            rich_cache = base_style_cache.clone().with_markdown_rich_overrides();
            &rich_cache
        } else {
            base_style_cache
        };

        // CST path: use tree-sitter with full AST context
        // Use injection-aware highlighting for SFC languages (Svelte, Vue)
        let line_highlights = if ext == "svelte" || ext == "vue" || ext == "md" || ext == "markdown" {
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

    // Post-process: hide/replace markdown syntax in rich mode
    let mut lines = lines;
    if markdown_rich && (ext == "md" || ext == "markdown") {
        apply_markdown_rich_transforms(&mut lines, &mut interner);
        apply_markdown_table_transforms(&mut lines, &mut interner);
    }

    DiffCache {
        file_index: 0, // Caller should update this
        patch_hash: hash_string(patch),
        lines,
        interner,
        highlighted: true,
        markdown_rich,
    }
}

/// Transform markdown syntax characters in rich display mode.
///
/// Uses sentinel colors from `ThemeStyleCache::with_markdown_rich_overrides()` to
/// distinguish block-level punctuation (heading/list markers) from inline-level
/// punctuation (emphasis/code delimiters) for added/context lines.
///
/// For removed lines (which go through the syntect fallback and lack sentinel colors),
/// applies text-based pattern matching to ensure consistent rendering.
///
/// - Heading markers (`#`, `##`, etc.) → removed
/// - Emphasis/strong delimiters (`*`, `**`, `_`) → removed
/// - List markers (`-`, `+`, `*`) → replaced with `・`
fn apply_markdown_rich_transforms(lines: &mut [CachedDiffLine], interner: &mut Rodeo) {
    use crate::syntax::themes::{MARKDOWN_BLOCK_PUNCT_COLOR, MARKDOWN_INLINE_PUNCT_COLOR};

    let bullet_spur = interner.get_or_intern("・");
    let bullet_space_spur = interner.get_or_intern("・ ");

    for line in lines.iter_mut() {
        if line.spans.len() <= 1 {
            continue;
        }

        // First span is the diff marker (+, -, space). Skip non-diff lines (hunk headers, etc.)
        let first = interner.resolve(&line.spans[0].content);
        let is_removed = first == "-";
        if first != "+" && first != "-" && first != " " {
            continue;
        }

        if is_removed {
            // Removed lines go through syntect fallback and don't have sentinel colors.
            // Apply text-based transforms for consistent rendering with added/context lines.
            apply_markdown_rich_transforms_text_based(line, interner, bullet_spur, bullet_space_spur);
        } else {
            // Added/context lines have sentinel colors from CST highlighting.
            apply_markdown_rich_transforms_sentinel(
                line,
                interner,
                bullet_spur,
                bullet_space_spur,
                MARKDOWN_BLOCK_PUNCT_COLOR,
                MARKDOWN_INLINE_PUNCT_COLOR,
            );
        }
    }
}

/// Apply markdown rich transforms using sentinel colors (for added/context lines).
fn apply_markdown_rich_transforms_sentinel(
    line: &mut CachedDiffLine,
    interner: &mut Rodeo,
    bullet_spur: lasso::Spur,
    bullet_space_spur: lasso::Spur,
    block_punct_color: Color,
    inline_punct_color: Color,
) {
    let mut removals: Vec<usize> = Vec::new();
    let mut replacements: Vec<(usize, lasso::Spur)> = Vec::new();

    let span_count = line.spans.len();
    for i in 1..span_count {
        let content = interner.resolve(&line.spans[i].content).to_string();
        let fg = line.spans[i].style.fg;

        if fg == Some(block_punct_color) {
            // Block-level punctuation: heading markers, list markers, blockquote
            if content.chars().all(|c| c == '#') && !content.is_empty() {
                // Heading marker (#, ##, ###, ...) → remove
                removals.push(i);
                // Also remove the trailing space separator
                if i + 1 < span_count {
                    let next = interner.resolve(&line.spans[i + 1].content);
                    if next == " " {
                        removals.push(i + 1);
                    }
                }
            } else if content.trim() == "-"
                || content.trim() == "+"
                || content.trim() == "*"
            {
                // List marker (may include trailing space) → replace with ・
                let spur = if content.ends_with(' ') {
                    bullet_space_spur
                } else {
                    bullet_spur
                };
                replacements.push((i, spur));
            }
            // Other block punctuation (>, thematic break, etc.) left as-is
        } else if fg == Some(inline_punct_color) {
            // Inline-level punctuation: emphasis/strong/code delimiters
            if content.chars().all(|c| c == '*' || c == '_') && !content.is_empty() {
                // Emphasis/strong delimiters (*, **, _, __) → remove
                removals.push(i);
            }
            // Code span/fence delimiters (`, ```) left as-is
        }
    }

    // Apply replacements
    for (i, spur) in &replacements {
        line.spans[*i].content = *spur;
        // Reset sentinel color to visible default
        line.spans[*i].style = Style::default();
    }

    // Apply removals in reverse order to preserve indices
    removals.sort_unstable();
    removals.dedup();
    for i in removals.into_iter().rev() {
        line.spans.remove(i);
    }

    // Normalize any remaining sentinel-colored spans back to a visible style.
    // Spans intentionally left as-is (backticks, code-fence delimiters, blockquote
    // markers, etc.) still carry sentinel colors which are nearly invisible.
    for span in line.spans[1..].iter_mut() {
        if span.style.fg == Some(block_punct_color)
            || span.style.fg == Some(inline_punct_color)
        {
            span.style = span.style.fg(Color::DarkGray);
        }
    }
}

/// Apply markdown rich transforms using text-based pattern matching (for removed lines).
///
/// Removed lines go through syntect and don't carry sentinel colors, so we detect
/// markdown patterns by examining the text content directly. This ensures removed
/// lines get the same visual treatment as added/context lines.
fn apply_markdown_rich_transforms_text_based(
    line: &mut CachedDiffLine,
    interner: &mut Rodeo,
    bullet_spur: lasso::Spur,
    bullet_space_spur: lasso::Spur,
) {
    // Reconstruct the full content (after the diff marker) to detect line-level patterns
    let full_content: String = line.spans[1..]
        .iter()
        .map(|s| interner.resolve(&s.content))
        .collect();
    let trimmed = full_content.trim_start();

    // Detect and apply heading marker removal (# at start of line)
    if let Some(rest) = trimmed.strip_prefix('#') {
        // Count consecutive # characters
        let hash_count = 1 + rest.chars().take_while(|c| *c == '#').count();
        let after_hashes = &trimmed[hash_count..];
        // Valid heading: # followed by space (or end of line)
        if after_hashes.is_empty() || after_hashes.starts_with(' ') {
            let prefix_to_remove = if after_hashes.starts_with(' ') {
                hash_count + 1 // hashes + space
            } else {
                hash_count
            };
            remove_leading_chars_from_spans(line, interner, prefix_to_remove);
        }
    }
    // Detect and apply list marker replacement (- / + / * at start of line)
    else if let Some(first_char) = trimmed.chars().next() {
        if (first_char == '-' || first_char == '+' || first_char == '*')
            && trimmed.len() > 1
            && trimmed.chars().nth(1) == Some(' ')
        {
            replace_leading_marker_with_bullet(
                line,
                interner,
                bullet_spur,
                bullet_space_spur,
            );
        }
    }

    // Remove inline emphasis/strong delimiters (* / ** / _ / __)
    remove_inline_emphasis_delimiters(line, interner);
}

/// Remove a given number of leading content characters from spans (after diff marker).
fn remove_leading_chars_from_spans(
    line: &mut CachedDiffLine,
    interner: &mut Rodeo,
    chars_to_remove: usize,
) {
    let mut remaining = chars_to_remove;
    let mut removals: Vec<usize> = Vec::new();

    for i in 1..line.spans.len() {
        if remaining == 0 {
            break;
        }
        let content = interner.resolve(&line.spans[i].content).to_string();
        // Skip leading whitespace spans (they are indentation, not heading markers)
        if removals.is_empty() && content.chars().all(|c| c.is_whitespace()) && !content.is_empty() {
            continue;
        }
        let char_count = content.chars().count();
        if char_count <= remaining {
            removals.push(i);
            remaining -= char_count;
        } else {
            // Partial removal: trim the beginning of this span
            let new_content: String = content.chars().skip(remaining).collect();
            line.spans[i].content = interner.get_or_intern(&new_content);
            remaining = 0;
        }
    }

    for i in removals.into_iter().rev() {
        line.spans.remove(i);
    }
}

/// Replace the leading list marker (- / + / *) with a bullet character.
fn replace_leading_marker_with_bullet(
    line: &mut CachedDiffLine,
    interner: &mut Rodeo,
    bullet_spur: lasso::Spur,
    bullet_space_spur: lasso::Spur,
) {
    for i in 1..line.spans.len() {
        let content = interner.resolve(&line.spans[i].content).to_string();
        // Skip whitespace-only spans (indentation)
        if content.chars().all(|c| c.is_whitespace()) && !content.is_empty() {
            continue;
        }
        let trimmed = content.trim_start();
        if let Some(first) = trimmed.chars().next() {
            if first == '-' || first == '+' || first == '*' {
                // Replace marker character(s) with bullet
                if trimmed.starts_with("- ") || trimmed.starts_with("+ ") || trimmed.starts_with("* ") {
                    let leading_ws: String = content.chars().take_while(|c| c.is_whitespace()).collect();
                    let after_marker: String = trimmed.chars().skip(2).collect();
                    if leading_ws.is_empty() && after_marker.is_empty() {
                        line.spans[i].content = bullet_space_spur;
                    } else {
                        let new_content = format!("{}・ {}", leading_ws, after_marker);
                        line.spans[i].content = interner.get_or_intern(&new_content);
                    }
                } else {
                    line.spans[i].content = bullet_spur;
                }
            }
        }
        break; // Only process the first non-whitespace span
    }
}

/// Remove inline emphasis/strong delimiters (* / ** / _ / __) from spans.
fn remove_inline_emphasis_delimiters(line: &mut CachedDiffLine, interner: &mut Rodeo) {
    for span in line.spans[1..].iter_mut() {
        let content = interner.resolve(&span.content).to_string();
        // Only process spans that contain emphasis delimiters mixed with content
        if content.contains('*') || content.contains('_') {
            // Remove standalone * or ** or _ or __ delimiters
            // Be careful not to remove * in content like "pointer *p" or underscores in identifiers
            if content.chars().all(|c| c == '*' || c == '_') && !content.is_empty() {
                // Pure delimiter span (e.g., "**", "*", "__", "_") → remove content
                span.content = interner.get_or_intern("");
            }
        }
    }
}

/// Transform markdown table rows to use box-drawing characters.
///
/// - Data/header rows: `|` → `│`
/// - Separator rows: `|` → `├`/`┼`/`┤`, `-` → `─`
/// - Header row (first row before separator): bold styling
fn apply_markdown_table_transforms(lines: &mut [CachedDiffLine], interner: &mut Rodeo) {
    // First pass: identify which lines are table lines and find header rows
    let mut table_line_info: Vec<Option<TableLineKind>> = Vec::with_capacity(lines.len());
    for line in lines.iter() {
        if line.spans.len() <= 1 {
            table_line_info.push(None);
            continue;
        }

        let first = interner.resolve(&line.spans[0].content);
        if first != "+" && first != "-" && first != " " {
            table_line_info.push(None);
            continue;
        }

        // Reconstruct content after diff marker
        let full_content: String = line.spans[1..]
            .iter()
            .map(|s| interner.resolve(&s.content))
            .collect();
        let trimmed = full_content.trim_start();

        if !trimmed.starts_with('|') {
            table_line_info.push(None);
            continue;
        }

        let is_separator =
            !trimmed.is_empty() && trimmed.chars().all(|c| c == '|' || c == '-' || c == ':' || c == ' ');

        if is_separator {
            table_line_info.push(Some(TableLineKind::Separator));
        } else {
            table_line_info.push(Some(TableLineKind::Data));
        }
    }

    // Mark header rows (data row immediately before a separator)
    let mut header_indices: Vec<usize> = Vec::new();
    for (i, info) in table_line_info.iter().enumerate() {
        if matches!(info, Some(TableLineKind::Separator)) {
            // Look back for the preceding data row
            if i > 0 {
                if let Some(TableLineKind::Data) = table_line_info[i - 1] {
                    header_indices.push(i - 1);
                }
            }
        }
    }

    // Second pass: transform spans
    for (i, line) in lines.iter_mut().enumerate() {
        let Some(kind) = &table_line_info[i] else {
            continue;
        };

        match kind {
            TableLineKind::Separator => {
                // Reconstruct full content and replace with box-drawing separator
                let full_content: String = line.spans[1..]
                    .iter()
                    .map(|s| interner.resolve(&s.content))
                    .collect();

                let separator = build_table_separator(&full_content);
                let style = Style::default().fg(Color::DarkGray);

                // Keep diff marker, replace rest with single styled span
                line.spans.truncate(1);
                line.spans.push(InternedSpan {
                    content: interner.get_or_intern(&separator),
                    style,
                });
            }
            TableLineKind::Data => {
                let is_header = header_indices.contains(&i);

                // Replace | with │ in each span
                for span in line.spans[1..].iter_mut() {
                    let content = interner.resolve(&span.content);
                    if content.contains('|') {
                        let new_content = content.replace('|', "│");
                        span.content = interner.get_or_intern(&new_content);
                    }
                    // Bold header cells
                    if is_header {
                        span.style = span.style.add_modifier(Modifier::BOLD);
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum TableLineKind {
    Data,
    Separator,
}

/// Build a box-drawing separator line from a markdown table separator.
///
/// `| --- | --- |` → `├───┼───┤`
/// `| --- | ---`   → `├───┼───`  (no trailing pipe → no `┤`)
fn build_table_separator(content: &str) -> String {
    let trimmed = content.trim_start();
    let chars: Vec<char> = trimmed.chars().collect();

    if chars.is_empty() {
        return content.to_string();
    }

    // Find pipe positions
    let pipe_positions: Vec<usize> = chars
        .iter()
        .enumerate()
        .filter(|(_, c)| **c == '|')
        .map(|(i, _)| i)
        .collect();

    if pipe_positions.is_empty() {
        return content.to_string();
    }

    let first_pipe = pipe_positions[0];
    let last_pipe = *pipe_positions.last().unwrap();

    // Only treat the last pipe as a closing border (┤) if it's actually at the end
    // of the content (only whitespace after it). Otherwise it's a column separator (┼).
    let has_trailing_pipe = chars[last_pipe + 1..].iter().all(|c| c.is_whitespace());

    // Preserve leading whitespace from original content
    let leading_ws: String = content.chars().take_while(|c| c.is_whitespace()).collect();

    let mut result = leading_ws;
    for (i, c) in chars.iter().enumerate() {
        if *c == '|' {
            if i == first_pipe {
                result.push('├');
            } else if i == last_pipe && has_trailing_pipe {
                result.push('┤');
            } else {
                result.push('┼');
            }
        } else if *c == '-' {
            result.push('─');
        } else {
            result.push(*c); // spaces, colons
        }
    }
    result
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
    let cache = build_diff_cache(patch, filename, theme_name, &mut parser_pool, false);

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
    } else if app.is_local_mode() {
        "j/k/↑↓: move | M: markdown rich | Ctrl-d/u: page | ←/h/q: back"
    } else {
        "j/k/↑↓: move | n/N: next/prev comment | Enter: comments | M: markdown rich | Ctrl-d/u: page | ←/h/q: back"
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
        let cache = build_diff_cache(patch, "test.rs", "Dracula", &mut parser_pool, false);

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
        let cache = build_diff_cache(patch, "test.rs", "Dracula", &mut parser_pool, false);

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
        let cache = build_diff_cache(patch, "test.ts", "Dracula", &mut parser_pool, false);

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
        let cache = build_diff_cache(patch, "Component.vue", "Dracula", &mut parser_pool, false);

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
        let highlighted = build_diff_cache(patch, "foo.rs", "base16-ocean.dark", &mut parser_pool, false);

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
        let highlighted = build_diff_cache(patch, "foo.rs", "base16-ocean.dark", &mut parser_pool, false);

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
            false,
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
            false,
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
            false,
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
        let cache = build_diff_cache(patch, "data.yaml", "base16-ocean.dark", &mut parser_pool, false);

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
        let cache = build_diff_cache(patch, "file.unknown", "base16-ocean.dark", &mut parser_pool, false);

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

    #[test]
    fn test_build_diff_cache_markdown_rich_flag() {
        let patch = r#"@@ -1,3 +1,4 @@
 # Heading
+## New Heading
 Some text
 More text"#;

        let mut parser_pool = ParserPool::new();

        // markdown_rich = false
        let cache_normal = build_diff_cache(patch, "README.md", "base16-ocean.dark", &mut parser_pool, false);
        assert!(!cache_normal.markdown_rich);
        assert!(cache_normal.highlighted);

        // markdown_rich = true
        let cache_rich = build_diff_cache(patch, "README.md", "base16-ocean.dark", &mut parser_pool, true);
        assert!(cache_rich.markdown_rich);
        assert!(cache_rich.highlighted);
    }

    #[test]
    fn test_build_diff_cache_markdown_rich_non_markdown() {
        // markdown_rich should be stored even for non-markdown files
        let patch = r#"@@ -1,2 +1,3 @@
 fn main() {}
+fn foo() {}
 fn bar() {}"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(patch, "test.rs", "base16-ocean.dark", &mut parser_pool, true);
        // Flag is stored regardless of file type
        assert!(cache.markdown_rich);
    }

    #[test]
    fn test_build_diff_cache_markdown_rich_styles_differ() {
        // Verify that markdown_rich=true actually changes styles for md files
        let patch = r#"@@ -1,2 +1,3 @@
 # Heading
+## New Heading
 Some text"#;

        let mut parser_pool = ParserPool::new();
        let cache_normal = build_diff_cache(patch, "README.md", "base16-ocean.dark", &mut parser_pool, false);
        let cache_rich = build_diff_cache(patch, "README.md", "base16-ocean.dark", &mut parser_pool, true);

        // Both should be highlighted
        assert!(cache_normal.highlighted);
        assert!(cache_rich.highlighted);
        assert!(!cache_normal.markdown_rich);
        assert!(cache_rich.markdown_rich);

        // They should have the same number of lines
        assert_eq!(cache_normal.lines.len(), cache_rich.lines.len());
    }

    #[test]
    fn test_build_diff_cache_markdown_injection_path() {
        // This test verifies the injection path (ext == "md") is taken
        let patch = r#"@@ -1,3 +1,5 @@
 # Title
+
+```rust
+fn main() {}
+```"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(patch, "test.md", "base16-ocean.dark", &mut parser_pool, false);
        assert!(cache.highlighted);
        assert!(!cache.lines.is_empty());
    }

    #[test]
    fn test_build_diff_cache_markdown_extension_variant() {
        // Test ".markdown" extension uses the same path as ".md"
        let patch = r#"@@ -1,2 +1,3 @@
 # Title
+Some **bold** text
 End"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(patch, "doc.markdown", "base16-ocean.dark", &mut parser_pool, false);
        assert!(cache.highlighted);
        assert!(!cache.lines.is_empty());
    }

    #[test]
    fn test_build_plain_diff_cache_has_no_markdown_rich() {
        let patch = r#"@@ -1,2 +1,2 @@
 # Heading
-old text
+new text"#;

        let cache = build_plain_diff_cache(patch);
        assert!(!cache.highlighted);
        assert!(!cache.markdown_rich);
    }

    /// Format a DiffCache into a readable snapshot string.
    /// Each line shows: [line_idx] span_count | "content" (style_info) ...
    fn format_diff_cache_spans(cache: &DiffCache) -> String {
        use ratatui::style::Modifier;

        cache
            .lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let spans: Vec<String> = line
                    .spans
                    .iter()
                    .map(|span| {
                        let content = cache.resolve(span.content);
                        let mut style_parts = Vec::new();
                        if let Some(fg) = span.style.fg {
                            style_parts.push(format!("fg:{:?}", fg));
                        }
                        if span.style.add_modifier.contains(Modifier::BOLD) {
                            style_parts.push("BOLD".to_string());
                        }
                        if span.style.add_modifier.contains(Modifier::ITALIC) {
                            style_parts.push("ITALIC".to_string());
                        }
                        if span.style.add_modifier.contains(Modifier::UNDERLINED) {
                            style_parts.push("UNDERLINED".to_string());
                        }
                        let style_str = if style_parts.is_empty() {
                            "default".to_string()
                        } else {
                            style_parts.join(",")
                        };
                        format!("{:?} [{}]", content, style_str)
                    })
                    .collect();
                format!("L{}: {}", i, spans.join(" | "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_snapshot_plain_diff_cache_markdown() {
        use insta::assert_snapshot;

        let patch = r#"@@ -1,3 +1,4 @@
 # Heading
+## New Section
 Some text
 More text"#;

        let cache = build_plain_diff_cache(patch);
        assert_snapshot!("plain_diff_cache_markdown", format_diff_cache_spans(&cache));
    }

    #[test]
    fn test_snapshot_highlighted_diff_cache_markdown() {
        use insta::assert_snapshot;

        let patch = r#"@@ -1,3 +1,4 @@
 # Heading
+## New Section
 Some text
 More text"#;

        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(patch, "README.md", "base16-ocean.dark", &mut parser_pool, false);

        assert!(cache.highlighted);
        assert_snapshot!("highlighted_diff_cache_markdown", format_diff_cache_spans(&cache));
    }

    #[test]
    fn test_snapshot_markdown_rich_vs_normal() {
        use insta::assert_snapshot;

        let patch = r#"@@ -1,2 +1,3 @@
 # Title
+**bold text**
 plain text"#;

        let mut parser_pool = ParserPool::new();
        let cache_normal = build_diff_cache(patch, "test.md", "base16-ocean.dark", &mut parser_pool, false);
        let cache_rich = build_diff_cache(patch, "test.md", "base16-ocean.dark", &mut parser_pool, true);

        let snapshot_normal = format_diff_cache_spans(&cache_normal);
        let snapshot_rich = format_diff_cache_spans(&cache_rich);

        // Both should produce valid output
        assert!(!snapshot_normal.is_empty());
        assert!(!snapshot_rich.is_empty());

        // Snapshot the rich mode output for regression detection
        assert_snapshot!("markdown_rich_mode", snapshot_rich);
    }

    #[test]
    fn test_build_table_separator() {
        // Spaces are preserved from the original separator format
        assert_eq!(
            build_table_separator("| --- | --- |"),
            "├ ─── ┼ ─── ┤"
        );
        assert_eq!(build_table_separator("| --- |"), "├ ─── ┤");
        assert_eq!(
            build_table_separator("| --- | --- | --- |"),
            "├ ─── ┼ ─── ┼ ─── ┤"
        );
        // Compact format without spaces
        assert_eq!(build_table_separator("|---|---|"), "├───┼───┤");
        // No trailing pipe: last pipe is a column separator, not a closing border
        assert_eq!(
            build_table_separator("| --- | ---"),
            "├ ─── ┼ ───"
        );
        assert_eq!(build_table_separator("|---|---"), "├───┼───");
        // Single column, no trailing pipe
        assert_eq!(build_table_separator("| ---"), "├ ───");
    }

    #[test]
    fn test_markdown_table_transforms() {
        use insta::assert_snapshot;

        let patch = r#"@@ -1,5 +1,5 @@
+| Name | Value |
+| --- | --- |
+| foo | 123 |
+| bar | 456 |
 plain text"#;

        let mut parser_pool = ParserPool::new();
        let cache =
            build_diff_cache(patch, "test.md", "base16-ocean.dark", &mut parser_pool, true);

        assert_snapshot!("markdown_table_rich", format_diff_cache_spans(&cache));
    }

    #[test]
    fn test_markdown_list_markers_replaced() {
        use insta::assert_snapshot;

        let patch = r#"@@ -1,4 +1,4 @@
+- item one
+* item two
++ item three
 plain text"#;

        let mut parser_pool = ParserPool::new();
        let cache =
            build_diff_cache(patch, "test.md", "base16-ocean.dark", &mut parser_pool, true);

        assert_snapshot!("markdown_list_rich", format_diff_cache_spans(&cache));
    }
}
