//! Unified highlighter supporting both tree-sitter and syntect.
//!
//! Tree-sitter is used for supported languages:
//! - Rust, TypeScript/TSX, JavaScript/JSX, Go, Python (original)
//! - Ruby, Zig, C, C++, Java, C# (added)
//! - Lua, Bash/Shell, PHP, Swift, Haskell (Phase 1)
//! - MoonBit (Phase 2)
//!
//! Syntect is used as a fallback for other languages (Vue, Svelte, YAML, Markdown, etc.).

use std::collections::HashMap;

use lasso::Rodeo;
use ratatui::style::Style;
use syntect::easy::HighlightLines;
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use crate::app::InternedSpan;
use crate::language::SupportedLanguage;

use super::parser_pool::ParserPool;
use super::themes::ThemeStyleCache;
use super::{convert_syntect_style, get_theme, syntax_for_file, syntax_set};

/// Unified highlighter that can use either tree-sitter or syntect.
///
/// This type does not hold any mutable references to `ParserPool`, allowing
/// the pool to be borrowed again during injection processing (e.g., for Svelte).
///
/// Query compilation is deferred to `ParserPool::get_or_create_query()` for caching.
pub enum Highlighter {
    /// Tree-sitter CST highlighter for supported languages.
    Cst {
        /// The supported language (used to look up parser/query from pool)
        supported_lang: SupportedLanguage,
        /// Pre-computed style cache from the theme.
        style_cache: ThemeStyleCache,
    },
    /// Syntect regex-based highlighter for fallback.
    Syntect(HighlightLines<'static>),
    /// No highlighting available.
    None,
}

/// A single highlight capture with byte range and style.
#[derive(Clone, Debug)]
pub struct LineCapture {
    /// Start byte offset within the line (relative to line start)
    pub local_start: usize,
    /// End byte offset within the line (relative to line start)
    pub local_end: usize,
    /// Style for this capture
    pub style: Style,
}

/// Pre-computed highlights grouped by source line index.
///
/// This allows O(1) lookup of highlights per line, avoiding repeated tree traversal.
pub struct LineHighlights {
    /// Map from source line index to captures in that line
    captures_by_line: HashMap<usize, Vec<LineCapture>>,
}

impl LineHighlights {
    /// Create an empty LineHighlights.
    pub fn empty() -> Self {
        Self {
            captures_by_line: HashMap::new(),
        }
    }

    /// Get captures for a specific line index.
    pub fn get(&self, line_index: usize) -> Option<&[LineCapture]> {
        self.captures_by_line.get(&line_index).map(|v| v.as_slice())
    }
}

/// Parsed tree-sitter result.
///
/// Query is not included here - it should be obtained from `ParserPool::get_or_create_query()`
/// to benefit from query caching.
pub struct CstParseResult {
    pub tree: Tree,
    /// The language that was parsed (use this to get cached query from ParserPool)
    pub lang: SupportedLanguage,
}

impl Highlighter {
    /// Create a highlighter for the given filename.
    ///
    /// Attempts to use tree-sitter first, falling back to syntect if the language
    /// is not supported by tree-sitter.
    ///
    /// Note: This does not borrow `ParserPool`. The parser is borrowed only during
    /// `parse_source`, allowing the pool to be used for injection processing.
    pub fn for_file(filename: &str, theme_name: &str) -> Self {
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        // Try tree-sitter first
        if let Some(supported_lang) = SupportedLanguage::from_extension(ext) {
            // Create style cache from theme for O(1) lookups
            // Query compilation is deferred to parse_source() via ParserPool cache
            let theme = get_theme(theme_name);
            let style_cache = ThemeStyleCache::new(theme);
            return Highlighter::Cst {
                supported_lang,
                style_cache,
            };
        }

        // Fall back to syntect
        if let Some(syntax) = syntax_for_file(filename) {
            let theme = get_theme(theme_name);
            return Highlighter::Syntect(HighlightLines::new(syntax, theme));
        }

        Highlighter::None
    }

    /// Parse the entire source and return a tree for line-by-line highlighting.
    ///
    /// For CST highlighter, parses the source and returns the tree.
    /// For Syntect, this is a no-op (syntect processes line by line).
    ///
    /// # Arguments
    /// * `source` - The source code to parse
    /// * `parser_pool` - The parser pool to borrow a parser from (borrowed only for this call)
    pub fn parse_source(
        &self,
        source: &str,
        parser_pool: &mut ParserPool,
    ) -> Option<CstParseResult> {
        match self {
            Highlighter::Cst {
                supported_lang,
                style_cache: _,
            } => {
                // Get parser for this language
                let parser = parser_pool.get_or_create(supported_lang.default_extension())?;
                let tree = parser.parse(source, None)?;
                Some(CstParseResult {
                    tree,
                    lang: *supported_lang,
                })
            }
            _ => None,
        }
    }

    /// Get a reference to the style cache for CST highlighting.
    ///
    /// Returns `None` for Syntect or None variants.
    pub fn style_cache(&self) -> Option<&ThemeStyleCache> {
        match self {
            Highlighter::Cst { style_cache, .. } => Some(style_cache),
            _ => None,
        }
    }

    /// Highlight a single line of code.
    ///
    /// For CST, this should be called after parse_source() with the tree.
    /// For Syntect, this can be called directly.
    pub fn highlight_line(&mut self, line: &str, interner: &mut Rodeo) -> Vec<InternedSpan> {
        match self {
            Highlighter::Cst { .. } => {
                // CST highlighting requires the tree, use highlight_line_with_tree instead
                vec![InternedSpan {
                    content: interner.get_or_intern(line),
                    style: Style::default(),
                }]
            }
            Highlighter::Syntect(hl) => highlight_with_syntect(line, hl, interner),
            Highlighter::None => {
                vec![InternedSpan {
                    content: interner.get_or_intern(line),
                    style: Style::default(),
                }]
            }
        }
    }
}

/// Collect all highlights from the tree in a single pass.
///
/// This runs the query once over the entire tree and groups captures by line,
/// avoiding the O(N * tree_size) cost of querying per-line.
///
/// # Arguments
/// * `source` - The complete source code
/// * `tree` - The parsed tree
/// * `query` - The highlight query
/// * `capture_names` - The capture names from the query
/// * `style_cache` - Pre-computed style cache from the theme
///
/// # Returns
/// A `LineHighlights` struct with captures grouped by source line index.
pub fn collect_line_highlights(
    source: &str,
    tree: &Tree,
    query: &Query,
    capture_names: &[String],
    style_cache: &ThemeStyleCache,
) -> LineHighlights {
    let mut cursor = QueryCursor::new();
    let mut captures_by_line: HashMap<usize, Vec<LineCapture>> = HashMap::new();

    // Pre-compute line byte offsets for fast line lookup
    let line_offsets: Vec<usize> =
        std::iter::once(0)
            .chain(source.bytes().enumerate().filter_map(|(i, b)| {
                if b == b'\n' {
                    Some(i + 1)
                } else {
                    None
                }
            }))
            .collect();

    // Run query once over the entire tree
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    while let Some(mat) = matches.next() {
        for capture in mat.captures {
            let node = capture.node;
            let start_byte = node.start_byte();
            let end_byte = node.end_byte();

            // Find which line(s) this capture spans
            let start_line = line_offsets
                .binary_search(&start_byte)
                .unwrap_or_else(|i| i.saturating_sub(1));

            let end_line = line_offsets
                .binary_search(&end_byte)
                .unwrap_or_else(|i| i.saturating_sub(1));

            let capture_name = &capture_names[capture.index as usize];
            let style = style_cache.get(capture_name);

            // Skip captures with no style (e.g., raw_text, which would mask other captures)
            if style == Style::default() {
                continue;
            }

            // Add capture to each line it spans
            for line_idx in start_line..=end_line {
                let line_start = line_offsets.get(line_idx).copied().unwrap_or(0);
                let line_end = line_offsets
                    .get(line_idx + 1)
                    .map(|&off| off.saturating_sub(1))
                    .unwrap_or(source.len());

                // Clamp capture to line boundaries
                let local_start = start_byte.saturating_sub(line_start);
                let local_end = end_byte
                    .saturating_sub(line_start)
                    .min(line_end - line_start);

                if local_start < local_end {
                    captures_by_line
                        .entry(line_idx)
                        .or_default()
                        .push(LineCapture {
                            local_start,
                            local_end,
                            style,
                        });
                }
            }
        }
    }

    // Sort captures within each line by start position
    for captures in captures_by_line.values_mut() {
        captures.sort_by_key(|c| c.local_start);
    }

    LineHighlights { captures_by_line }
}

/// Collect highlights with injection support for languages like Svelte.
///
/// This extends `collect_line_highlights` to handle embedded languages:
/// - Parses the parent language (e.g., Svelte)
/// - Extracts injection ranges using the language's injection query
/// - Highlights each injection range with the appropriate parser
/// - Merges all highlights into a single `LineHighlights`
///
/// # Arguments
/// * `source` - The complete source code
/// * `tree` - The parsed tree from the parent parser
/// * `query` - The highlight query for the parent language
/// * `capture_names` - Capture names from the parent query
/// * `style_cache` - Style cache for theme colors
/// * `parser_pool` - Parser pool for creating injection parsers and cached queries
/// * `parent_ext` - File extension of the parent language (e.g., "svelte")
pub fn collect_line_highlights_with_injections(
    source: &str,
    tree: &Tree,
    lang: SupportedLanguage,
    style_cache: &ThemeStyleCache,
    parser_pool: &mut ParserPool,
    parent_ext: &str,
) -> LineHighlights {
    use crate::syntax::injection::{extract_injections, normalize_language_name};

    // Get cached query for parent language
    let query = match parser_pool.get_or_create_query(lang) {
        Some(q) => q,
        None => return LineHighlights::empty(),
    };
    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Start with parent language highlights
    let mut result = collect_line_highlights(source, tree, query, &capture_names, style_cache);

    // Get parent language for injection query
    let parent_lang = match SupportedLanguage::from_extension(parent_ext) {
        Some(lang) => lang,
        None => return result,
    };

    // Get injection query for parent language
    let injection_query = match parent_ext {
        "svelte" => tree_sitter_svelte_ng::INJECTIONS_QUERY,
        "vue" => tree_sitter_vue3::INJECTIONS_QUERY,
        _ => return result, // No injection support for other languages yet
    };

    // Extract injection ranges
    let ts_language = parent_lang.ts_language();
    let injections = extract_injections(tree, source.as_bytes(), &ts_language, injection_query);

    if injections.is_empty() {
        return result;
    }

    // Pre-compute line byte offsets for fast line lookup
    let line_offsets: Vec<usize> =
        std::iter::once(0)
            .chain(source.bytes().enumerate().filter_map(|(i, b)| {
                if b == b'\n' {
                    Some(i + 1)
                } else {
                    None
                }
            }))
            .collect();

    // Process each injection
    for injection in injections {
        let mut normalized_lang = normalize_language_name(&injection.language);

        // Svelte/Vue injection query marks all raw_text as "javascript" by default,
        // but <style> content should be CSS. Use the parent node kind from the
        // syntax tree to determine the correct language.
        if normalized_lang == "javascript" && (parent_ext == "svelte" || parent_ext == "vue") {
            if let Some(ref parent_kind) = injection.parent_node_kind {
                // Check if this injection is inside a style element
                if parent_kind.contains("style") {
                    normalized_lang = "css";
                }
                // script_element keeps "javascript" (or typescript if lang attr is set)
            }
        }

        // Map normalized language name to file extension
        let ext = match normalized_lang {
            "typescript" => "ts",
            "javascript" => "js",
            "tsx" => "tsx", // Vue supports <script lang="tsx">
            "jsx" => "jsx", // Vue supports <script lang="jsx">
            "css" => "css",
            "html" => continue, // Skip HTML injections (handled by parent)
            _ => continue,      // Skip unsupported languages
        };

        // Get the injection content
        let inj_source = &source[injection.range.clone()];

        // Get injection language
        let Some(inj_lang) = SupportedLanguage::from_extension(ext) else {
            continue;
        };

        // Parse the injection content (scoped to release parser borrow)
        let inj_tree = match parser_pool.get_or_create(ext) {
            Some(parser) => match parser.parse(inj_source, None) {
                Some(tree) => tree,
                None => continue,
            },
            None => continue,
        };
        // parser_pool borrow is released here

        // Get cached highlight query for injection language
        let Some(inj_query) = parser_pool.get_or_create_query(inj_lang) else {
            continue;
        };

        let inj_capture_names: Vec<String> = inj_query
            .capture_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Collect highlights from injection
        let mut inj_cursor = QueryCursor::new();
        let mut inj_matches =
            inj_cursor.matches(inj_query, inj_tree.root_node(), inj_source.as_bytes());

        while let Some(mat) = inj_matches.next() {
            for capture in mat.captures {
                let node = capture.node;
                // Byte offsets are relative to injection source
                let local_start = node.start_byte();
                let local_end = node.end_byte();

                // Convert to absolute byte offset in full source
                let abs_start = injection.range.start + local_start;
                let abs_end = injection.range.start + local_end;

                // Find which line(s) this capture spans
                let start_line = line_offsets
                    .binary_search(&abs_start)
                    .unwrap_or_else(|i| i.saturating_sub(1));

                let end_line = line_offsets
                    .binary_search(&abs_end)
                    .unwrap_or_else(|i| i.saturating_sub(1));

                let capture_name = &inj_capture_names[capture.index as usize];
                let style = style_cache.get(capture_name);

                // Skip captures with no style
                if style == Style::default() {
                    continue;
                }

                // Add capture to each line it spans
                for line_idx in start_line..=end_line {
                    let line_start = line_offsets.get(line_idx).copied().unwrap_or(0);
                    let line_end = line_offsets
                        .get(line_idx + 1)
                        .map(|&off| off.saturating_sub(1))
                        .unwrap_or(source.len());

                    // Clamp capture to line boundaries (relative to line start)
                    let cap_local_start = abs_start.saturating_sub(line_start);
                    let cap_local_end = abs_end
                        .saturating_sub(line_start)
                        .min(line_end - line_start);

                    if cap_local_start < cap_local_end {
                        result
                            .captures_by_line
                            .entry(line_idx)
                            .or_default()
                            .push(LineCapture {
                                local_start: cap_local_start,
                                local_end: cap_local_end,
                                style,
                            });
                    }
                }
            }
        }
    }

    // Re-sort captures within each line by start position
    // For captures starting at the same position, longer captures come first so that
    // shorter (more specific) captures can override them when we process in order
    for captures in result.captures_by_line.values_mut() {
        captures.sort_by(|a, b| {
            a.local_start.cmp(&b.local_start).then_with(|| {
                // Sort by length descending (longer first) so shorter captures
                // are processed later and override longer ones
                (b.local_end - b.local_start).cmp(&(a.local_end - a.local_start))
            })
        });
    }

    result
}

/// Apply pre-computed highlights to a line, producing InternedSpans.
///
/// When captures overlap, the more specific (shorter) capture takes precedence.
/// This allows injection highlights to override parent language highlights.
///
/// # Arguments
/// * `line` - The line content
/// * `captures` - Pre-computed captures for this line (from `collect_line_highlights`)
/// * `interner` - String interner for deduplication
pub fn apply_line_highlights(
    line: &str,
    captures: Option<&[LineCapture]>,
    interner: &mut Rodeo,
) -> Vec<InternedSpan> {
    let captures = match captures {
        Some(c) if !c.is_empty() => c,
        _ => {
            // No highlights, return plain text
            return vec![InternedSpan {
                content: interner.get_or_intern(line),
                style: Style::default(),
            }];
        }
    };

    // Build spans using an event-based approach instead of byte-map for better performance.
    // This is O(m log m) where m is the number of captures, rather than O(n) where n is line length.
    // For long lines (e.g., minified code), this is much more efficient.

    // Collect boundary events: (position, is_start, capture_index)
    // We'll process these in order to build spans
    let mut events: Vec<(usize, bool, usize)> = Vec::with_capacity(captures.len() * 2);

    for (idx, capture) in captures.iter().enumerate() {
        // Skip invalid captures
        if capture.local_start >= capture.local_end || capture.local_end > line.len() {
            continue;
        }
        events.push((capture.local_start, true, idx)); // start event
        events.push((capture.local_end, false, idx)); // end event
    }

    // If no valid captures, return plain text
    if events.is_empty() {
        return vec![InternedSpan {
            content: interner.get_or_intern(line),
            style: Style::default(),
        }];
    }

    // Sort events by position, with end events before start events at same position
    events.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| {
            // End events (false) come before start events (true) at same position
            a.1.cmp(&b.1)
        })
    });

    // Build spans by tracking active captures
    // Use a stack approach: shorter captures (higher specificity) override longer ones
    let mut spans = Vec::new();
    let mut active_captures: Vec<usize> = Vec::new(); // indices of currently active captures
    let mut last_pos = 0;

    for (pos, is_start, capture_idx) in events {
        // Emit span for the gap before this event if there's content
        if pos > last_pos {
            let style = active_captures
                .last()
                .map(|&idx| captures[idx].style)
                .unwrap_or_default();
            let text = &line[last_pos..pos];
            if !text.is_empty() {
                spans.push(InternedSpan {
                    content: interner.get_or_intern(text),
                    style,
                });
            }
        }

        if is_start {
            // Push new capture - shorter captures are processed after longer ones
            // (due to sorting in collect_line_highlights), so they'll be on top
            active_captures.push(capture_idx);
        } else {
            // Remove this capture from active set
            if let Some(idx) = active_captures.iter().rposition(|&c| c == capture_idx) {
                active_captures.remove(idx);
            }
        }

        last_pos = pos;
    }

    // Emit final span if there's remaining content
    if last_pos < line.len() {
        let style = active_captures
            .last()
            .map(|&idx| captures[idx].style)
            .unwrap_or_default();
        let text = &line[last_pos..];
        if !text.is_empty() {
            spans.push(InternedSpan {
                content: interner.get_or_intern(text),
                style,
            });
        }
    }

    // If no spans were created, return the whole line as plain text
    if spans.is_empty() {
        spans.push(InternedSpan {
            content: interner.get_or_intern(line),
            style: Style::default(),
        });
    }

    spans
}

/// Highlight a line using syntect.
fn highlight_with_syntect(
    line: &str,
    hl: &mut HighlightLines<'_>,
    interner: &mut Rodeo,
) -> Vec<InternedSpan> {
    match hl.highlight_line(line, syntax_set()) {
        Ok(ranges) => ranges
            .into_iter()
            .map(|(style, text)| InternedSpan {
                content: interner.get_or_intern(text),
                style: convert_syntect_style(&style),
            })
            .collect(),
        Err(_) => {
            vec![InternedSpan {
                content: interner.get_or_intern(line),
                style: Style::default(),
            }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlighter_rust() {
        let highlighter = Highlighter::for_file("test.rs", "base16-ocean.dark");
        assert!(
            matches!(highlighter, Highlighter::Cst { .. }),
            "Expected Cst highlighter for Rust"
        );
    }

    #[test]
    fn test_highlighter_typescript() {
        let highlighter = Highlighter::for_file("test.ts", "base16-ocean.dark");
        assert!(matches!(highlighter, Highlighter::Cst { .. }));
    }

    #[test]
    fn test_highlighter_vue_cst() {
        // Vue is now supported with tree-sitter (Phase 3c)
        let highlighter = Highlighter::for_file("test.vue", "base16-ocean.dark");
        assert!(
            matches!(highlighter, Highlighter::Cst { .. }),
            "Expected Cst highlighter for Vue"
        );
    }

    #[test]
    fn test_highlighter_yaml_fallback() {
        let highlighter = Highlighter::for_file("test.yaml", "base16-ocean.dark");
        assert!(matches!(highlighter, Highlighter::Syntect(_)));
    }

    #[test]
    fn test_highlighter_unknown() {
        let highlighter = Highlighter::for_file("test.unknown", "base16-ocean.dark");
        assert!(matches!(highlighter, Highlighter::None));
    }

    #[test]
    fn test_cst_parse_and_highlight() {
        use crate::syntax::get_theme;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.rs", "base16-ocean.dark");

        let source = "fn main() {\n    let x = 42;\n}";

        if let Some(result) = highlighter.parse_source(source, &mut pool) {
            // Get cached query
            let query = pool.get_or_create_query(result.lang).unwrap();
            let capture_names: Vec<String> = query
                .capture_names()
                .iter()
                .map(|s| s.to_string())
                .collect();

            // Create style cache from theme for testing
            let theme = get_theme("base16-ocean.dark");
            let style_cache = ThemeStyleCache::new(theme);
            let line_highlights =
                collect_line_highlights(source, &result.tree, query, &capture_names, &style_cache);

            let mut interner = Rodeo::default();
            let line = "fn main() {";
            let captures = line_highlights.get(0);
            let spans = apply_line_highlights(line, captures, &mut interner);

            // Should have multiple spans with different styles
            assert!(!spans.is_empty());

            // Check that "main" is highlighted as a function name
            let main_text = spans
                .iter()
                .find(|s| interner.resolve(&s.content) == "main");
            assert!(main_text.is_some(), "Should highlight 'main' function name");
        }
    }

    #[test]
    fn test_syntect_highlight() {
        let mut highlighter = Highlighter::for_file("test.vue", "base16-ocean.dark");

        let mut interner = Rodeo::default();
        let spans = highlighter.highlight_line("<template>", &mut interner);

        assert!(!spans.is_empty());
    }

    #[test]
    fn test_cst_with_dracula_theme() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;
        use ratatui::style::Color;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.rs", "Dracula");

        let source = "fn main() {\n    let x = 42;\n}";

        // Parse source (borrows pool only for this call)
        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse Rust source");

        // Get cached query
        let query = pool.get_or_create_query(result.lang).unwrap();
        let capture_names: Vec<String> = query
            .capture_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Create style cache from Dracula theme
        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights =
            collect_line_highlights(source, &result.tree, query, &capture_names, &style_cache);

        let mut interner = Rodeo::default();
        let line = "fn main() {";
        let captures = line_highlights.get(0);
        let spans = apply_line_highlights(line, captures, &mut interner);

        // "fn" should have Dracula pink color (keyword)
        let fn_span = spans.iter().find(|s| interner.resolve(&s.content) == "fn");
        assert!(fn_span.is_some(), "Should have 'fn' span");

        let fn_style = fn_span.unwrap().style;
        // Dracula keyword color is Rgb(255, 121, 198) (pink)
        match fn_style.fg {
            Some(Color::Rgb(r, g, b)) => {
                // Dracula pink is approximately Rgb(255, 121, 198)
                assert!(
                    r > 200 && g < 200 && b > 150,
                    "Expected Dracula pink-ish color for 'fn', got Rgb({}, {}, {})",
                    r,
                    g,
                    b
                );
            }
            other => {
                panic!("Expected Rgb color for 'fn' keyword, got {:?}", other);
            }
        }
    }

    #[test]
    fn test_use_keyword_with_dracula_theme() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;
        use ratatui::style::Color;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.rs", "Dracula");

        let source = "use std::collections::HashMap;\n\nfn main() {}";

        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse Rust source");

        // Get cached query
        let query = pool.get_or_create_query(result.lang).unwrap();
        let capture_names: Vec<String> = query
            .capture_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights =
            collect_line_highlights(source, &result.tree, query, &capture_names, &style_cache);

        let mut interner = Rodeo::default();
        let line = "use std::collections::HashMap;";
        let captures = line_highlights.get(0);
        let spans = apply_line_highlights(line, captures, &mut interner);

        // "use" should have Dracula pink color (keyword)
        let use_span = spans.iter().find(|s| interner.resolve(&s.content) == "use");
        assert!(use_span.is_some(), "Should have 'use' span");

        let use_style = use_span.unwrap().style;
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
    fn test_vue_primed_highlighting() {
        use syntect::easy::HighlightLines;
        use syntect::highlighting::Color;

        let tf_ss = two_face::syntax::extra_newlines();
        let syntax = tf_ss.find_syntax_by_extension("vue").unwrap();
        let theme = crate::syntax::get_theme("Dracula");

        // Test with priming (our fix)
        let mut hl = HighlightLines::new(syntax, theme);
        // Prime with virtual <script> tag
        let _ = hl.highlight_line("<script lang=\"ts\">\n", &tf_ss);

        // Now highlight code without actual <script> tag in diff
        let regions = hl
            .highlight_line("const onClickPageName = () => {\n", &tf_ss)
            .unwrap();

        // Find the "const" token
        let const_region = regions.iter().find(|(_, text)| *text == "const");
        assert!(const_region.is_some(), "Should find 'const' token");

        // Dracula cyan is approximately (139, 233, 253)
        let (style, _) = const_region.unwrap();
        let Color { r, g, b, .. } = style.foreground;
        assert!(
            r < 200 && g > 200 && b > 200,
            "const should be cyan-ish, got ({}, {}, {})",
            r,
            g,
            b
        );

        // Find the function name
        let func_region = regions.iter().find(|(_, text)| *text == "onClickPageName");
        assert!(func_region.is_some(), "Should find 'onClickPageName' token");

        // Dracula green is approximately (80, 250, 123)
        let (style, _) = func_region.unwrap();
        let Color { r, g, b, .. } = style.foreground;
        assert!(
            r < 150 && g > 200 && b < 200,
            "onClickPageName should be green-ish, got ({}, {}, {})",
            r,
            g,
            b
        );
    }

    #[test]
    fn test_typescript_function_highlighting() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.ts", "Dracula");

        // Arrow function assignment - common pattern in Vue/React
        let source = "const onClickPageName = () => {\n  const rootDom = store.tree\n}";

        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse TypeScript source");

        // Get cached query
        let query = pool.get_or_create_query(result.lang).unwrap();
        let capture_names: Vec<String> = query
            .capture_names()
            .iter()
            .map(|s| s.to_string())
            .collect();

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights =
            collect_line_highlights(source, &result.tree, query, &capture_names, &style_cache);

        let mut interner = Rodeo::default();
        let line = "const onClickPageName = () => {";
        let captures = line_highlights.get(0);
        let spans = apply_line_highlights(line, captures, &mut interner);

        // "const" should be highlighted as keyword
        let const_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "const");
        assert!(const_span.is_some(), "Should have 'const' span");
        assert!(
            const_span.unwrap().style.fg.is_some(),
            "'const' should have foreground color"
        );

        // "onClickPageName" should be highlighted as function
        let func_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "onClickPageName");
        assert!(
            func_span.is_some(),
            "Should have 'onClickPageName' span (function name)"
        );
        assert!(
            func_span.unwrap().style.fg.is_some(),
            "'onClickPageName' should have foreground color (function)"
        );
    }

    #[test]
    fn test_svelte_uses_cst_highlighter() {
        let highlighter = Highlighter::for_file("test.svelte", "Dracula");
        assert!(
            matches!(highlighter, Highlighter::Cst { .. }),
            "Svelte should use CST highlighter"
        );
    }

    #[test]
    fn test_svelte_script_injection_typescript() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.svelte", "Dracula");

        // Svelte file with TypeScript in <script>
        let source = r#"<script lang="ts">
    const count: number = 0;
    function increment() {
        count += 1;
    }
</script>

<button on:click={increment}>
    {count}
</button>"#;

        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse Svelte source");

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        // Use injection-aware highlighting
        let line_highlights = collect_line_highlights_with_injections(
            source,
            &result.tree,
            result.lang,
            &style_cache,
            &mut pool,
            "svelte",
        );

        let mut interner = Rodeo::default();

        // Line 2: "    const count: number = 0;"
        // "const" should be highlighted as keyword (TypeScript injection)
        let line = "    const count: number = 0;";
        let captures = line_highlights.get(1); // Line index 1
        let spans = apply_line_highlights(line, captures, &mut interner);

        let const_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "const");
        assert!(
            const_span.is_some(),
            "Should find 'const' in script injection"
        );
        assert!(
            const_span.unwrap().style.fg.is_some(),
            "'const' should have syntax highlighting from TypeScript parser"
        );

        // "number" should be highlighted as type
        let number_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "number");
        assert!(
            number_span.is_some(),
            "Should find 'number' type in script injection"
        );
        assert!(
            number_span.unwrap().style.fg.is_some(),
            "'number' should have syntax highlighting as type"
        );
    }

    #[test]
    fn test_svelte_style_injection_css() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.svelte", "Dracula");

        // Svelte file with CSS in <style>
        let source = r#"<script>
    let visible = true;
</script>

<style>
    .container {
        color: red;
        display: flex;
    }
</style>

<div class="container">Hello</div>"#;

        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse Svelte source");

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights = collect_line_highlights_with_injections(
            source,
            &result.tree,
            result.lang,
            &style_cache,
            &mut pool,
            "svelte",
        );

        let mut interner = Rodeo::default();

        // Line 6: "    .container {"
        let line = "    .container {";
        let captures = line_highlights.get(5); // Line index 5
        let spans = apply_line_highlights(line, captures, &mut interner);

        // ".container" or "container" should be highlighted as CSS class selector
        let has_class_highlight = spans.iter().any(|s| {
            let text = interner.resolve(&s.content);
            (text == ".container" || text == "container") && s.style.fg.is_some()
        });
        assert!(
            has_class_highlight,
            "CSS class selector should be highlighted in style injection"
        );

        // Line 7: "        color: red;"
        let line = "        color: red;";
        let captures = line_highlights.get(6);
        let spans = apply_line_highlights(line, captures, &mut interner);

        // "color" should be highlighted as CSS property
        let color_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "color");
        assert!(
            color_span.is_some(),
            "Should find 'color' CSS property in style injection"
        );
        assert!(
            color_span.unwrap().style.fg.is_some(),
            "'color' should have syntax highlighting as CSS property"
        );
    }

    /// Test that script blocks containing `<style` substring are NOT misclassified as CSS.
    ///
    /// This is a regression test for the issue where raw string search (`rfind("<style")`)
    /// would incorrectly detect `<style` inside JavaScript code (e.g., template strings,
    /// comments, DOM manipulation) and apply CSS highlighting to the script block.
    #[test]
    fn test_svelte_script_with_style_substring_not_misclassified() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.svelte", "Dracula");

        // Svelte file with script containing "<style" as a string literal
        // This should NOT be misclassified as CSS
        let source = r#"<script lang="ts">
    const template = `<style>body { color: red; }</style>`;
    const element = document.querySelector("<style");
    function addStyle() {
        const style = "<style>test</style>";
        return style;
    }
</script>

<style>
    .real-css { color: blue; }
</style>

<div>{template}</div>"#;

        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse Svelte source");

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights = collect_line_highlights_with_injections(
            source,
            &result.tree,
            result.lang,
            &style_cache,
            &mut pool,
            "svelte",
        );

        let mut interner = Rodeo::default();

        // Line 2: "    const template = `<style>body { color: red; }</style>`;"
        // "const" should be highlighted as JS/TS keyword (NOT CSS)
        let line = "    const template = `<style>body { color: red; }</style>`;";
        let captures = line_highlights.get(1); // Line index 1
        let spans = apply_line_highlights(line, captures, &mut interner);

        let const_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "const");
        assert!(
            const_span.is_some(),
            "Should find 'const' in script block with <style substring"
        );
        assert!(
            const_span.unwrap().style.fg.is_some(),
            "'const' should be highlighted as keyword (TypeScript), not misclassified as CSS"
        );

        // Line 4: "    function addStyle() {"
        // "function" should be highlighted as JS/TS keyword
        let line = "    function addStyle() {";
        let captures = line_highlights.get(3); // Line index 3
        let spans = apply_line_highlights(line, captures, &mut interner);

        let function_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "function");
        assert!(
            function_span.is_some(),
            "Should find 'function' in script block"
        );
        assert!(
            function_span.unwrap().style.fg.is_some(),
            "'function' should be highlighted as keyword"
        );

        // Line 11: "    .real-css { color: blue; }"
        // This is actual CSS in a <style> block, should be highlighted as CSS
        let line = "    .real-css { color: blue; }";
        let captures = line_highlights.get(10); // Line index 10
        let spans = apply_line_highlights(line, captures, &mut interner);

        // "color" in the actual CSS block should be highlighted
        let color_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "color");
        assert!(
            color_span.is_some(),
            "Should find 'color' in actual CSS block"
        );
        assert!(
            color_span.unwrap().style.fg.is_some(),
            "'color' in real <style> block should be highlighted as CSS property"
        );
    }

    #[test]
    fn test_vue_script_injection_typescript() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.vue", "Dracula");

        // Vue file with TypeScript in <script>
        let source = r#"<script lang="ts">
    const count: number = 0;
    function increment() {
        count += 1;
    }
</script>

<template>
    <button @click="increment">
        {{ count }}
    </button>
</template>"#;

        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse Vue source");

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        // Use injection-aware highlighting
        let line_highlights = collect_line_highlights_with_injections(
            source,
            &result.tree,
            result.lang,
            &style_cache,
            &mut pool,
            "vue",
        );

        let mut interner = Rodeo::default();

        // Line 2: "    const count: number = 0;"
        // "const" should be highlighted as keyword (TypeScript injection)
        let line = "    const count: number = 0;";
        let captures = line_highlights.get(1);
        let spans = apply_line_highlights(line, captures, &mut interner);

        // "const" is part of "    const " span due to how captures overlap
        let const_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content).contains("const"));
        assert!(
            const_span.is_some(),
            "Should find span containing 'const' in TypeScript script block"
        );
        assert!(
            const_span.unwrap().style.fg.is_some(),
            "'const' should be highlighted as keyword in Vue TypeScript block"
        );
    }

    #[test]
    fn test_vue_style_injection_css() {
        use crate::syntax::get_theme;
        use crate::syntax::themes::ThemeStyleCache;

        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.vue", "Dracula");

        // Vue file with CSS in <style>
        let source = r#"<template>
    <div class="container">Hello</div>
</template>

<style>
    .container {
        color: red;
    }
</style>"#;

        let result = highlighter
            .parse_source(source, &mut pool)
            .expect("Should parse Vue source");

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights = collect_line_highlights_with_injections(
            source,
            &result.tree,
            result.lang,
            &style_cache,
            &mut pool,
            "vue",
        );

        let mut interner = Rodeo::default();

        // Line 7: "        color: red;"
        let line = "        color: red;";
        let captures = line_highlights.get(6);
        let spans = apply_line_highlights(line, captures, &mut interner);

        // "color" should be highlighted as CSS property
        let color_span = spans
            .iter()
            .find(|s| interner.resolve(&s.content) == "color");
        assert!(
            color_span.is_some(),
            "Should find 'color' CSS property in Vue style injection"
        );
        assert!(
            color_span.unwrap().style.fg.is_some(),
            "'color' should have syntax highlighting as CSS property in Vue"
        );
    }
}

#[cfg(test)]
mod priming_injection_tests {
    use super::*;
    use crate::language::SupportedLanguage;
    use crate::syntax::get_theme;

    #[test]
    fn test_collect_highlights_primed_vue() {
        // Simulate primed source: wrapping plain script content in <script lang="ts">
        let source = r#"<script lang="ts">
import { ref } from 'vue'
const count = ref(0)
</script>
"#;

        // Parse with Vue parser
        let mut pool = ParserPool::new();
        let parser = pool.get_or_create("vue").unwrap();
        let tree = parser.parse(source, None).unwrap();

        // Get style cache
        let theme_name = "base16-ocean.dark";
        let theme = get_theme(theme_name);
        let style_cache = ThemeStyleCache::new(theme);

        // Collect highlights with injection
        let highlights = collect_line_highlights_with_injections(
            source,
            &tree,
            SupportedLanguage::Vue,
            &style_cache,
            &mut pool,
            "vue",
        );

        // Line 1 is import, line 2 is const (line 0 is <script lang="ts">)
        let line1_captures = highlights.get(1);
        let line2_captures = highlights.get(2);

        assert!(
            line1_captures.is_some() && !line1_captures.unwrap().is_empty(),
            "Should have highlights for import line"
        );
        assert!(
            line2_captures.is_some() && !line2_captures.unwrap().is_empty(),
            "Should have highlights for const line"
        );
    }
}
