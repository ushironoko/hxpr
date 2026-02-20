//! Parser pool for efficient tree-sitter parser management.
//!
//! Parsers are relatively heavy objects (~200KB each), so we maintain a pool
//! to reuse them across multiple files rather than creating new ones for each file.
//!
//! Also caches compiled queries, which are expensive to create (they require
//! parsing the query string and building internal data structures).

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use tree_sitter::{Parser, Query};

use crate::language::SupportedLanguage;

/// Pool of tree-sitter parsers and compiled queries, one per language.
///
/// Parsers and queries are lazily created on first use and reused for subsequent operations.
/// This avoids the overhead of creating new parsers/compiling queries for each file.
pub struct ParserPool {
    parsers: HashMap<SupportedLanguage, Parser>,
    /// Cached compiled queries for highlight queries
    queries: HashMap<SupportedLanguage, Query>,
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
            queries: HashMap::new(),
        }
    }

    /// Get or create a compiled highlight query for the given language.
    ///
    /// Queries are cached to avoid recompilation overhead on each use.
    /// This is particularly important for injection processing (e.g., Svelte)
    /// where multiple queries are needed per file.
    pub fn get_or_create_query(&mut self, lang: SupportedLanguage) -> Option<&Query> {
        // If query doesn't exist, compile it
        if let Entry::Vacant(e) = self.queries.entry(lang) {
            let ts_language = lang.ts_language();
            let query_source = lang.highlights_query();
            let query = Query::new(&ts_language, query_source).ok()?;
            e.insert(query);
        }

        self.queries.get(&lang)
    }

    /// Get or create a parser for the given file extension.
    ///
    /// Returns `None` if the extension is not supported by tree-sitter.
    pub fn get_or_create(&mut self, ext: &str) -> Option<&mut Parser> {
        let lang = SupportedLanguage::from_extension(ext)?;

        // If parser doesn't exist, create it
        if let Entry::Vacant(e) = self.parsers.entry(lang) {
            let ts_language = lang.ts_language();
            let mut parser = Parser::new();
            if parser.set_language(&ts_language).is_err() {
                return None;
            }
            e.insert(parser);
        }

        self.parsers.get_mut(&lang)
    }

    /// Check if tree-sitter supports the given file extension.
    pub fn supports_extension(ext: &str) -> bool {
        SupportedLanguage::is_supported(ext)
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
    fn test_parser_pool_ruby() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("rb").is_some());
        assert!(pool.get_or_create("rake").is_some());
        assert!(pool.get_or_create("gemspec").is_some());
    }

    #[test]
    fn test_parser_pool_zig() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("zig").is_some());
    }

    #[test]
    fn test_parser_pool_c() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("c").is_some());
        // .h files are treated as C
        assert!(pool.get_or_create("h").is_some());
    }

    #[test]
    fn test_parser_pool_cpp() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("cpp").is_some());
        assert!(pool.get_or_create("cc").is_some());
        assert!(pool.get_or_create("cxx").is_some());
        assert!(pool.get_or_create("hpp").is_some());
        assert!(pool.get_or_create("hxx").is_some());
    }

    #[test]
    fn test_parser_pool_java() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("java").is_some());
    }

    #[test]
    fn test_parser_pool_csharp() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("cs").is_some());
    }

    #[test]
    fn test_parser_pool_lua() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("lua").is_some());
    }

    #[test]
    fn test_parser_pool_bash() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("sh").is_some());
        assert!(pool.get_or_create("bash").is_some());
        assert!(pool.get_or_create("zsh").is_some());
    }

    #[test]
    fn test_parser_pool_php() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("php").is_some());
    }

    #[test]
    fn test_parser_pool_swift() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("swift").is_some());
    }

    #[test]
    fn test_parser_pool_haskell() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("hs").is_some());
        assert!(pool.get_or_create("lhs").is_some());
    }

    // Svelte falls back to syntect (tree-sitter-svelte-ng requires injection)

    #[test]
    fn test_parser_pool_moonbit() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("mbt").is_some());
    }

    #[test]
    fn test_parser_pool_svelte() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("svelte").is_some());
    }

    #[test]
    fn test_parser_pool_vue() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("vue").is_some());
    }

    #[test]
    fn test_parser_pool_markdown() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("md").is_some());
        assert!(pool.get_or_create("markdown").is_some());
    }

    #[test]
    fn test_parser_pool_markdown_inline() {
        let mut pool = ParserPool::new();
        assert!(pool.get_or_create("md_inline").is_some());
    }

    #[test]
    fn test_parser_pool_unsupported() {
        let mut pool = ParserPool::new();
        // Vue is now supported in Phase 3c
        assert!(pool.get_or_create("yaml").is_none());
        assert!(pool.get_or_create("toml").is_none());
    }

    #[test]
    fn test_supports_extension() {
        // Original languages
        assert!(ParserPool::supports_extension("rs"));
        assert!(ParserPool::supports_extension("ts"));
        assert!(ParserPool::supports_extension("tsx"));
        assert!(ParserPool::supports_extension("js"));
        assert!(ParserPool::supports_extension("jsx"));
        assert!(ParserPool::supports_extension("go"));
        assert!(ParserPool::supports_extension("py"));

        // Phase 1 languages
        assert!(ParserPool::supports_extension("lua"));
        assert!(ParserPool::supports_extension("sh"));
        assert!(ParserPool::supports_extension("php"));
        assert!(ParserPool::supports_extension("swift"));
        assert!(ParserPool::supports_extension("hs"));

        // Phase 3: Svelte is now supported
        assert!(ParserPool::supports_extension("svelte"));
        // Phase 3c: Vue is now supported
        assert!(ParserPool::supports_extension("vue"));

        // Phase 4: Markdown
        assert!(ParserPool::supports_extension("md"));

        assert!(!ParserPool::supports_extension("yaml"));
    }

    #[test]
    fn test_parser_can_parse() {
        let mut pool = ParserPool::new();
        let parser = pool.get_or_create("rs").unwrap();

        let code = "fn main() { println!(\"Hello\"); }";
        let tree = parser.parse(code, None);
        assert!(tree.is_some(), "Should parse Rust code");
    }

    #[test]
    fn test_query_cache_creates_query() {
        let mut pool = ParserPool::new();
        let query = pool.get_or_create_query(SupportedLanguage::Rust);
        assert!(query.is_some(), "Should create Rust highlight query");
    }

    #[test]
    fn test_query_cache_reuses_query() {
        let mut pool = ParserPool::new();

        // First call creates the query
        let query1 = pool.get_or_create_query(SupportedLanguage::TypeScript);
        assert!(query1.is_some());

        // Second call should return the same query (cached)
        let query2 = pool.get_or_create_query(SupportedLanguage::TypeScript);
        assert!(query2.is_some());

        // Verify cache is populated
        assert!(
            pool.queries.contains_key(&SupportedLanguage::TypeScript),
            "Query should be cached"
        );
    }

    #[test]
    fn test_query_cache_multiple_languages() {
        let mut pool = ParserPool::new();

        // Create queries for multiple languages
        assert!(pool.get_or_create_query(SupportedLanguage::Rust).is_some());
        assert!(pool
            .get_or_create_query(SupportedLanguage::TypeScript)
            .is_some());
        assert!(pool
            .get_or_create_query(SupportedLanguage::JavaScript)
            .is_some());
        assert!(pool.get_or_create_query(SupportedLanguage::Css).is_some());

        // All should be cached
        assert_eq!(pool.queries.len(), 4);
    }

    #[test]
    fn test_query_has_capture_names() {
        let mut pool = ParserPool::new();
        let query = pool.get_or_create_query(SupportedLanguage::Rust).unwrap();

        // Query should have capture names for highlighting
        let capture_names = query.capture_names();
        assert!(!capture_names.is_empty(), "Query should have capture names");
    }

    #[test]
    fn test_query_cache_markdown() {
        let mut pool = ParserPool::new();
        let query = pool.get_or_create_query(SupportedLanguage::Markdown);
        assert!(query.is_some(), "Should compile Markdown block highlight query");
    }

    #[test]
    fn test_query_cache_markdown_inline() {
        let mut pool = ParserPool::new();
        let query = pool.get_or_create_query(SupportedLanguage::MarkdownInline);
        assert!(query.is_some(), "Should compile MarkdownInline highlight query");
    }

    #[test]
    fn test_markdown_parser_can_parse() {
        let mut pool = ParserPool::new();
        let parser = pool.get_or_create("md").unwrap();

        let code = "# Hello\n\nSome **bold** text.\n";
        let tree = parser.parse(code, None);
        assert!(tree.is_some(), "Should parse Markdown code");
    }

    #[test]
    fn test_supports_extension_markdown() {
        assert!(ParserPool::supports_extension("md"));
        assert!(ParserPool::supports_extension("markdown"));
        assert!(ParserPool::supports_extension("md_inline"));
    }
}
