//! Parser pool for efficient tree-sitter parser management.
//!
//! Parsers are relatively heavy objects (~200KB each), so we maintain a pool
//! to reuse them across multiple files rather than creating new ones for each file.

use std::collections::HashMap;
use tree_sitter::{Language, Parser};

/// Pool of tree-sitter parsers, one per language.
///
/// Parsers are lazily created on first use and reused for subsequent parses.
/// This avoids the overhead of creating a new parser for each file.
pub struct ParserPool {
    parsers: HashMap<&'static str, Parser>,
}

impl Default for ParserPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ParserPool {
    /// Create a new empty parser pool.
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    /// Get or create a parser for the given file extension.
    ///
    /// Returns `None` if the extension is not supported by tree-sitter.
    pub fn get_or_create(&mut self, ext: &str) -> Option<&mut Parser> {
        // Map extension to a static key for the HashMap
        let key = match ext {
            "rs" => "rs",
            "ts" => "ts",
            "tsx" => "tsx",
            "js" => "js",
            "jsx" => "jsx",
            "go" => "go",
            "py" => "py",
            _ => return None,
        };

        // If parser doesn't exist, create it
        if !self.parsers.contains_key(key) {
            let language = get_language(key)?;
            let mut parser = Parser::new();
            if parser.set_language(&language).is_err() {
                return None;
            }
            self.parsers.insert(key, parser);
        }

        self.parsers.get_mut(key)
    }

    /// Check if tree-sitter supports the given file extension.
    pub fn supports_extension(ext: &str) -> bool {
        matches!(ext, "rs" | "ts" | "tsx" | "js" | "jsx" | "go" | "py")
    }
}

/// Get the tree-sitter Language for a file extension.
fn get_language(ext: &str) -> Option<Language> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_pool_rust() {
        let mut pool = ParserPool::new();
        let parser = pool.get_or_create("rs");
        assert!(parser.is_some(), "Should create Rust parser");

        // Second call should return the same parser
        let parser2 = pool.get_or_create("rs");
        assert!(parser2.is_some(), "Should reuse Rust parser");
    }

    #[test]
    fn test_parser_pool_typescript() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("ts").is_some());
        assert!(pool.get_or_create("tsx").is_some());
    }

    #[test]
    fn test_parser_pool_javascript() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("js").is_some());
        assert!(pool.get_or_create("jsx").is_some());
    }

    #[test]
    fn test_parser_pool_go() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("go").is_some());
    }

    #[test]
    fn test_parser_pool_python() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("py").is_some());
    }

    #[test]
    fn test_parser_pool_unsupported() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("vue").is_none());
        assert!(pool.get_or_create("yaml").is_none());
        assert!(pool.get_or_create("toml").is_none());
    }

    #[test]
    fn test_supports_extension() {
        assert!(ParserPool::supports_extension("rs"));
        assert!(ParserPool::supports_extension("ts"));
        assert!(ParserPool::supports_extension("tsx"));
        assert!(ParserPool::supports_extension("js"));
        assert!(ParserPool::supports_extension("jsx"));
        assert!(ParserPool::supports_extension("go"));
        assert!(ParserPool::supports_extension("py"));

        assert!(!ParserPool::supports_extension("vue"));
        assert!(!ParserPool::supports_extension("yaml"));
        assert!(!ParserPool::supports_extension("md"));
    }

    #[test]
    fn test_parser_can_parse() {
        let mut pool = ParserPool::new();
        let parser = pool.get_or_create("rs").unwrap();

        let code = "fn main() { println!(\"Hello\"); }";
        let tree = parser.parse(code, None);
        assert!(tree.is_some(), "Should parse Rust code");
    }
}
