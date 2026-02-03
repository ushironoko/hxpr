//! Unified highlighter supporting both tree-sitter and syntect.
//!
//! Tree-sitter is used for supported languages (Rust, TypeScript, JavaScript, Go, Python).
//! Syntect is used as a fallback for other languages (Vue, YAML, Markdown, etc.).

use std::collections::HashMap;

use lasso::Rodeo;
use ratatui::style::Style;
use syntect::easy::HighlightLines;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator, Tree};

use crate::app::InternedSpan;
use crate::language::SupportedLanguage;

use super::parser_pool::ParserPool;
use super::themes::ThemeStyleCache;
use super::{convert_syntect_style, get_theme, syntax_for_file, syntax_set};

/// Unified highlighter that can use either tree-sitter or syntect.
pub enum Highlighter<'a> {
    /// Tree-sitter CST highlighter for supported languages.
    Cst {
        parser: &'a mut Parser,
        language: Language,
        query_source: &'static str,
        capture_names: Vec<String>,
        /// Pre-computed style cache from the theme.
        style_cache: ThemeStyleCache,
    },
    /// Syntect regex-based highlighter for fallback.
    Syntect(HighlightLines<'a>),
    /// No highlighting available.
    None,
}

/// A single highlight capture with byte range and style.
#[derive(Clone)]
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
    /// Get captures for a specific line index.
    pub fn get(&self, line_index: usize) -> Option<&[LineCapture]> {
        self.captures_by_line.get(&line_index).map(|v| v.as_slice())
    }
}

/// Parsed tree-sitter result with query.
pub struct CstParseResult {
    pub tree: Tree,
    pub query: Query,
    pub capture_names: Vec<String>,
}

impl<'a> Highlighter<'a> {
    /// Create a highlighter for the given filename.
    ///
    /// Attempts to use tree-sitter first, falling back to syntect if the language
    /// is not supported by tree-sitter.
    pub fn for_file(filename: &str, theme_name: &str, parser_pool: &'a mut ParserPool) -> Self {
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        // Try tree-sitter first
        if let Some(lang) = SupportedLanguage::from_extension(ext) {
            if let Some(parser) = parser_pool.get_or_create(ext) {
                let language = lang.ts_language();
                let query_source = lang.highlights_query();
                // Pre-create query to get capture names
                if let Ok(query) = Query::new(&language, query_source) {
                    let capture_names = query
                        .capture_names()
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    // Create style cache from theme for O(1) lookups
                    let theme = get_theme(theme_name);
                    let style_cache = ThemeStyleCache::new(theme);
                    return Highlighter::Cst {
                        parser,
                        language,
                        query_source,
                        capture_names,
                        style_cache,
                    };
                }
            }
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
    pub fn parse_source(&mut self, source: &str) -> Option<CstParseResult> {
        match self {
            Highlighter::Cst {
                parser,
                language,
                query_source,
                capture_names,
                style_cache: _,
            } => {
                let tree = parser.parse(source, None)?;
                // Create a fresh query for the result
                let query = Query::new(language, query_source).ok()?;
                Some(CstParseResult {
                    tree,
                    query,
                    capture_names: capture_names.clone(),
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

/// Apply pre-computed highlights to a line, producing InternedSpans.
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

    let mut spans = Vec::new();
    let mut last_end = 0;

    for capture in captures {
        // Skip captures that overlap with already processed text
        // This handles duplicate captures from tree-sitter error recovery
        if capture.local_start < last_end {
            continue;
        }

        // Skip invalid captures
        if capture.local_start >= capture.local_end || capture.local_end > line.len() {
            continue;
        }

        // Add unstyled text before this highlight
        if capture.local_start > last_end {
            spans.push(InternedSpan {
                content: interner.get_or_intern(&line[last_end..capture.local_start]),
                style: Style::default(),
            });
        }

        // Add highlighted text
        spans.push(InternedSpan {
            content: interner.get_or_intern(&line[capture.local_start..capture.local_end]),
            style: capture.style,
        });
        last_end = capture.local_end;
    }

    // Add remaining unstyled text
    if last_end < line.len() {
        spans.push(InternedSpan {
            content: interner.get_or_intern(&line[last_end..]),
            style: Style::default(),
        });
    }

    // If no spans, return the whole line as plain text
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
        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.rs", "base16-ocean.dark", &mut pool);
        assert!(
            matches!(highlighter, Highlighter::Cst { .. }),
            "Expected Cst highlighter for Rust"
        );
    }

    #[test]
    fn test_highlighter_typescript() {
        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.ts", "base16-ocean.dark", &mut pool);
        assert!(matches!(highlighter, Highlighter::Cst { .. }));
    }

    #[test]
    fn test_highlighter_vue_fallback() {
        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.vue", "base16-ocean.dark", &mut pool);
        assert!(matches!(highlighter, Highlighter::Syntect(_)));
    }

    #[test]
    fn test_highlighter_yaml_fallback() {
        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.yaml", "base16-ocean.dark", &mut pool);
        assert!(matches!(highlighter, Highlighter::Syntect(_)));
    }

    #[test]
    fn test_highlighter_unknown() {
        let mut pool = ParserPool::new();
        let highlighter = Highlighter::for_file("test.unknown", "base16-ocean.dark", &mut pool);
        assert!(matches!(highlighter, Highlighter::None));
    }

    #[test]
    fn test_cst_parse_and_highlight() {
        use crate::syntax::get_theme;

        let mut pool = ParserPool::new();
        let mut highlighter = Highlighter::for_file("test.rs", "base16-ocean.dark", &mut pool);

        let source = "fn main() {\n    let x = 42;\n}";

        if let Some(result) = highlighter.parse_source(source) {
            // Create style cache from theme for testing
            let theme = get_theme("base16-ocean.dark");
            let style_cache = ThemeStyleCache::new(theme);
            let line_highlights = collect_line_highlights(
                source,
                &result.tree,
                &result.query,
                &result.capture_names,
                &style_cache,
            );

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
        let mut pool = ParserPool::new();
        let mut highlighter = Highlighter::for_file("test.vue", "base16-ocean.dark", &mut pool);

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
        let mut highlighter = Highlighter::for_file("test.rs", "Dracula", &mut pool);

        let source = "fn main() {\n    let x = 42;\n}";

        // Parse first (requires &mut self)
        let result = highlighter
            .parse_source(source)
            .expect("Should parse Rust source");

        // Create style cache from Dracula theme
        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights = collect_line_highlights(
            source,
            &result.tree,
            &result.query,
            &result.capture_names,
            &style_cache,
        );

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
        let mut highlighter = Highlighter::for_file("test.rs", "Dracula", &mut pool);

        let source = "use std::collections::HashMap;\n\nfn main() {}";

        let result = highlighter
            .parse_source(source)
            .expect("Should parse Rust source");

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights = collect_line_highlights(
            source,
            &result.tree,
            &result.query,
            &result.capture_names,
            &style_cache,
        );

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
        let mut highlighter = Highlighter::for_file("test.ts", "Dracula", &mut pool);

        // Arrow function assignment - common pattern in Vue/React
        let source = "const onClickPageName = () => {\n  const rootDom = store.tree\n}";

        let result = highlighter
            .parse_source(source)
            .expect("Should parse TypeScript source");

        let theme = get_theme("Dracula");
        let style_cache = ThemeStyleCache::new(theme);

        let line_highlights = collect_line_highlights(
            source,
            &result.tree,
            &result.query,
            &result.capture_names,
            &style_cache,
        );

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
}
