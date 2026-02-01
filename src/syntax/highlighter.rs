//! Unified highlighter supporting both tree-sitter and syntect.
//!
//! Tree-sitter is used for supported languages (Rust, TypeScript, JavaScript, Go, Python).
//! Syntect is used as a fallback for other languages (Vue, YAML, Markdown, etc.).

use lasso::Rodeo;
use ratatui::style::Style;
use syntect::easy::HighlightLines;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator, Tree};

use crate::app::InternedSpan;

use super::parser_pool::ParserPool;
use super::themes::style_for_capture;
use super::{convert_syntect_style, get_theme, syntax_for_file, syntax_set};

/// Unified highlighter that can use either tree-sitter or syntect.
pub enum Highlighter<'a> {
    /// Tree-sitter CST highlighter for supported languages.
    Cst {
        parser: &'a mut Parser,
        language: Language,
        query_source: &'static str,
        capture_names: Vec<String>,
    },
    /// Syntect regex-based highlighter for fallback.
    Syntect(HighlightLines<'a>),
    /// No highlighting available.
    None,
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
        if let Some(parser) = parser_pool.get_or_create(ext) {
            if let Some((language, query_source)) = get_language_and_query(ext) {
                // Pre-create query to get capture names
                if let Ok(query) = Query::new(&language, query_source) {
                    let capture_names = query
                        .capture_names()
                        .iter()
                        .map(|s| s.to_string())
                        .collect();
                    return Highlighter::Cst {
                        parser,
                        language,
                        query_source,
                        capture_names,
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

/// Highlight a line using CST tree.
///
/// # Arguments
/// * `source` - The complete source code
/// * `line` - The line content
/// * `line_start_byte` - Byte offset where this line starts in the source
/// * `line_end_byte` - Byte offset where this line ends in the source
/// * `tree` - The parsed tree
/// * `query` - The highlight query
/// * `capture_names` - The capture names from the query
/// * `interner` - String interner for deduplication
pub fn highlight_line_with_tree(
    source: &str,
    line: &str,
    line_start_byte: usize,
    line_end_byte: usize,
    tree: &Tree,
    query: &Query,
    capture_names: &[String],
    interner: &mut Rodeo,
) -> Vec<InternedSpan> {
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(line_start_byte..line_end_byte);

    let mut highlights: Vec<(usize, usize, Style)> = Vec::new();

    // Collect all captures in this line using matches
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    while let Some(mat) = matches.next() {
        for capture in mat.captures {
            let node = capture.node;
            let start = node.start_byte();
            let end = node.end_byte();

            // Clamp to line boundaries
            let local_start = start.saturating_sub(line_start_byte);
            let local_end = end.saturating_sub(line_start_byte).min(line.len());

            if local_start < local_end {
                let capture_name = &capture_names[capture.index as usize];
                let style = style_for_capture(capture_name);
                highlights.push((local_start, local_end, style));
            }
        }
    }

    // Sort by start position
    highlights.sort_by_key(|(start, _, _)| *start);

    // Build spans from highlights
    let mut spans = Vec::new();
    let mut last_end = 0;

    for (start, end, style) in highlights {
        // Add unstyled text before this highlight
        if start > last_end {
            spans.push(InternedSpan {
                content: interner.get_or_intern(&line[last_end..start]),
                style: Style::default(),
            });
        }

        // Add highlighted text
        if start < end && end <= line.len() {
            spans.push(InternedSpan {
                content: interner.get_or_intern(&line[start..end]),
                style,
            });
            last_end = end;
        }
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

/// Get the language and query source for a file extension.
fn get_language_and_query(ext: &str) -> Option<(Language, &'static str)> {
    match ext {
        "rs" => Some((
            tree_sitter_rust::LANGUAGE.into(),
            include_str!("../../queries/rust.scm"),
        )),
        "ts" => Some((
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            include_str!("../../queries/typescript.scm"),
        )),
        "tsx" => Some((
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            include_str!("../../queries/typescript.scm"),
        )),
        "js" | "jsx" => Some((
            tree_sitter_javascript::LANGUAGE.into(),
            include_str!("../../queries/javascript.scm"),
        )),
        "go" => Some((
            tree_sitter_go::LANGUAGE.into(),
            include_str!("../../queries/go.scm"),
        )),
        "py" => Some((
            tree_sitter_python::LANGUAGE.into(),
            include_str!("../../queries/python.scm"),
        )),
        _ => None,
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
        let mut pool = ParserPool::new();
        let mut highlighter = Highlighter::for_file("test.rs", "base16-ocean.dark", &mut pool);

        let source = "fn main() {\n    let x = 42;\n}";

        if let Some(result) = highlighter.parse_source(source) {
            let mut interner = Rodeo::default();
            let line = "fn main() {";
            let spans = highlight_line_with_tree(
                source,
                line,
                0,
                line.len(),
                &result.tree,
                &result.query,
                &result.capture_names,
                &mut interner,
            );

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
}
