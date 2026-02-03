//! Centralized language support definitions.
//!
//! This module provides a single source of truth for all language-specific
//! information used throughout the codebase, including:
//! - File extension to language mapping
//! - Tree-sitter language and highlight queries
//! - Definition keyword prefixes for Go to Definition
//! - Common keywords to exclude from symbol candidates

use std::sync::LazyLock;
use tree_sitter::Language;

/// Supported languages for tree-sitter based syntax highlighting and symbol analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    Rust,
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Go,
    Python,
}

/// Combined TypeScript highlights query (JavaScript base + TypeScript-specific).
///
/// TypeScript's HIGHLIGHTS_QUERY only contains TypeScript-specific patterns
/// (abstract, declare, enum, etc.) and expects to inherit JavaScript patterns.
/// We combine them here for complete highlighting.
static TYPESCRIPT_COMBINED_QUERY: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}\n{}",
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_typescript::HIGHLIGHTS_QUERY
    )
});

/// All definition prefixes from all supported languages, deduplicated.
static ALL_DEFINITION_PREFIXES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut prefixes = Vec::new();
    for lang in SupportedLanguage::all() {
        for prefix in lang.definition_prefixes() {
            if !prefixes.contains(prefix) {
                prefixes.push(*prefix);
            }
        }
    }
    prefixes
});

/// All keywords from all supported languages, deduplicated.
static ALL_KEYWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut keywords = Vec::new();
    for lang in SupportedLanguage::all() {
        for keyword in lang.keywords() {
            if !keywords.contains(keyword) {
                keywords.push(*keyword);
            }
        }
    }
    keywords
});

impl SupportedLanguage {
    /// Create a SupportedLanguage from a file extension.
    ///
    /// Returns `None` if the extension is not supported.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::TypeScriptReact),
            "js" => Some(Self::JavaScript),
            "jsx" => Some(Self::JavaScriptReact),
            "go" => Some(Self::Go),
            "py" => Some(Self::Python),
            _ => None,
        }
    }

    /// Check if the given file extension is supported.
    pub fn is_supported(ext: &str) -> bool {
        Self::from_extension(ext).is_some()
    }

    /// Get the tree-sitter Language for this language.
    pub fn ts_language(&self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::TypeScriptReact => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript | Self::JavaScriptReact => tree_sitter_javascript::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
        }
    }

    /// Get the highlights query for this language.
    ///
    /// For TypeScript/TSX, this returns a combined query including JavaScript
    /// patterns, since tree-sitter-typescript's query only contains TS-specific
    /// additions.
    pub fn highlights_query(&self) -> &'static str {
        match self {
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
            Self::TypeScript | Self::TypeScriptReact => {
                // SAFETY: LazyLock ensures the string is initialized once and lives
                // for the duration of the program
                TYPESCRIPT_COMBINED_QUERY.as_str()
            }
            Self::JavaScript | Self::JavaScriptReact => tree_sitter_javascript::HIGHLIGHT_QUERY,
            Self::Go => tree_sitter_go::HIGHLIGHTS_QUERY,
            Self::Python => tree_sitter_python::HIGHLIGHTS_QUERY,
        }
    }

    /// Get the definition keyword prefixes for this language.
    ///
    /// Each entry is a keyword (including trailing space) that precedes a symbol
    /// name in a definition context.
    pub fn definition_prefixes(&self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "fn ",
                "pub fn ",
                "pub(crate) fn ",
                "pub(super) fn ",
                "struct ",
                "pub struct ",
                "enum ",
                "pub enum ",
                "trait ",
                "pub trait ",
                "type ",
                "pub type ",
                "const ",
                "pub const ",
                "static ",
                "pub static ",
                "mod ",
                "pub mod ",
                "impl ",
                "impl<",
            ],
            Self::TypeScript | Self::TypeScriptReact | Self::JavaScript | Self::JavaScriptReact => {
                &[
                    "function ",
                    "export function ",
                    "class ",
                    "interface ",
                    "export class ",
                    "export interface ",
                    "export type ",
                    "export enum ",
                    "export const ",
                ]
            }
            Self::Python => &["def ", "class "],
            Self::Go => &["func ", "type ", "var "],
        }
    }

    /// Get common keywords for this language that should be excluded from
    /// symbol popup candidates.
    pub fn keywords(&self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "fn", "pub", "let", "mut", "const", "static", "struct", "enum", "trait", "impl",
                "mod", "use", "crate", "self", "super", "where", "for", "in", "if", "else",
                "match", "return", "break", "continue", "loop", "while", "as", "ref", "move",
                "async", "await", "dyn", "type", "true", "false", "Some", "None", "Ok", "Err",
                "Self",
            ],
            Self::TypeScript | Self::TypeScriptReact | Self::JavaScript | Self::JavaScriptReact => {
                &[
                    "function",
                    "class",
                    "interface",
                    "export",
                    "import",
                    "from",
                    "default",
                    "const",
                    "let",
                    "var",
                    "new",
                    "this",
                    "typeof",
                    "instanceof",
                    "void",
                    "null",
                    "undefined",
                    "try",
                    "catch",
                    "throw",
                    "finally",
                    "yield",
                    "delete",
                    "switch",
                    "case",
                    "if",
                    "else",
                    "for",
                    "while",
                    "return",
                    "break",
                    "continue",
                    "true",
                    "false",
                    "async",
                    "await",
                ]
            }
            Self::Python => &[
                "def", "class", "pass", "raise", "with", "lambda", "global", "nonlocal", "assert",
                "del", "not", "and", "or", "is", "elif", "except", "if", "else", "for", "while",
                "return", "break", "continue", "import", "from", "try", "except", "finally",
                "True", "False", "None",
            ],
            Self::Go => &[
                "func",
                "package",
                "defer",
                "go",
                "select",
                "chan",
                "fallthrough",
                "range",
                "map",
                "type",
                "var",
                "const",
                "struct",
                "interface",
                "if",
                "else",
                "for",
                "switch",
                "case",
                "return",
                "break",
                "continue",
                "import",
                "true",
                "false",
                "nil",
            ],
        }
    }

    /// Get all definition prefixes from all supported languages.
    ///
    /// Returns a deduplicated list of prefixes.
    pub fn all_definition_prefixes() -> &'static [&'static str] {
        &ALL_DEFINITION_PREFIXES
    }

    /// Get all keywords from all supported languages.
    ///
    /// Returns a deduplicated list of keywords.
    pub fn all_keywords() -> &'static [&'static str] {
        &ALL_KEYWORDS
    }

    /// Iterate over all supported languages.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::Rust,
            Self::TypeScript,
            Self::TypeScriptReact,
            Self::JavaScript,
            Self::JavaScriptReact,
            Self::Go,
            Self::Python,
        ]
        .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension_supported() {
        assert_eq!(
            SupportedLanguage::from_extension("rs"),
            Some(SupportedLanguage::Rust)
        );
        assert_eq!(
            SupportedLanguage::from_extension("ts"),
            Some(SupportedLanguage::TypeScript)
        );
        assert_eq!(
            SupportedLanguage::from_extension("tsx"),
            Some(SupportedLanguage::TypeScriptReact)
        );
        assert_eq!(
            SupportedLanguage::from_extension("js"),
            Some(SupportedLanguage::JavaScript)
        );
        assert_eq!(
            SupportedLanguage::from_extension("jsx"),
            Some(SupportedLanguage::JavaScriptReact)
        );
        assert_eq!(
            SupportedLanguage::from_extension("go"),
            Some(SupportedLanguage::Go)
        );
        assert_eq!(
            SupportedLanguage::from_extension("py"),
            Some(SupportedLanguage::Python)
        );
    }

    #[test]
    fn test_from_extension_unsupported() {
        assert_eq!(SupportedLanguage::from_extension("vue"), None);
        assert_eq!(SupportedLanguage::from_extension("yaml"), None);
        assert_eq!(SupportedLanguage::from_extension("md"), None);
        assert_eq!(SupportedLanguage::from_extension("toml"), None);
        assert_eq!(SupportedLanguage::from_extension(""), None);
    }

    #[test]
    fn test_is_supported() {
        assert!(SupportedLanguage::is_supported("rs"));
        assert!(SupportedLanguage::is_supported("ts"));
        assert!(SupportedLanguage::is_supported("tsx"));
        assert!(SupportedLanguage::is_supported("js"));
        assert!(SupportedLanguage::is_supported("jsx"));
        assert!(SupportedLanguage::is_supported("go"));
        assert!(SupportedLanguage::is_supported("py"));

        assert!(!SupportedLanguage::is_supported("vue"));
        assert!(!SupportedLanguage::is_supported("yaml"));
        assert!(!SupportedLanguage::is_supported("md"));
    }

    #[test]
    fn test_ts_language_is_valid() {
        // Verify each language can be converted to a tree-sitter Language
        for lang in SupportedLanguage::all() {
            let ts_lang = lang.ts_language();
            // Just ensure it doesn't panic
            assert!(ts_lang.abi_version() > 0);
        }
    }

    #[test]
    fn test_highlights_query_not_empty() {
        for lang in SupportedLanguage::all() {
            let query = lang.highlights_query();
            assert!(
                !query.is_empty(),
                "{:?} should have a non-empty highlights query",
                lang
            );
        }
    }

    #[test]
    fn test_typescript_combined_query() {
        let query = SupportedLanguage::TypeScript.highlights_query();
        // Should contain JavaScript patterns
        assert!(
            query.contains("function"),
            "TypeScript query should include JavaScript patterns"
        );
        // Should also contain TypeScript-specific patterns
        // Note: exact content depends on tree-sitter-typescript version
        assert!(
            query.len() > tree_sitter_javascript::HIGHLIGHT_QUERY.len(),
            "Combined query should be longer than JavaScript-only query"
        );
    }

    #[test]
    fn test_definition_prefixes_not_empty() {
        for lang in SupportedLanguage::all() {
            let prefixes = lang.definition_prefixes();
            assert!(
                !prefixes.is_empty(),
                "{:?} should have definition prefixes",
                lang
            );
        }
    }

    #[test]
    fn test_keywords_not_empty() {
        for lang in SupportedLanguage::all() {
            let keywords = lang.keywords();
            assert!(!keywords.is_empty(), "{:?} should have keywords", lang);
        }
    }

    #[test]
    fn test_all_definition_prefixes_deduplicated() {
        let prefixes = SupportedLanguage::all_definition_prefixes();
        let mut seen = std::collections::HashSet::new();
        for prefix in prefixes {
            assert!(seen.insert(*prefix), "Duplicate prefix found: {}", prefix);
        }
    }

    #[test]
    fn test_all_keywords_deduplicated() {
        let keywords = SupportedLanguage::all_keywords();
        let mut seen = std::collections::HashSet::new();
        for keyword in keywords {
            assert!(
                seen.insert(*keyword),
                "Duplicate keyword found: {}",
                keyword
            );
        }
    }

    #[test]
    fn test_all_definition_prefixes_contains_expected() {
        let prefixes = SupportedLanguage::all_definition_prefixes();
        // Check a few expected prefixes from different languages
        assert!(prefixes.contains(&"fn "), "Should contain Rust 'fn '");
        assert!(
            prefixes.contains(&"function "),
            "Should contain JS 'function '"
        );
        assert!(prefixes.contains(&"def "), "Should contain Python 'def '");
        assert!(prefixes.contains(&"func "), "Should contain Go 'func '");
    }

    #[test]
    fn test_all_keywords_contains_expected() {
        let keywords = SupportedLanguage::all_keywords();
        // Check a few expected keywords from different languages
        assert!(keywords.contains(&"fn"), "Should contain Rust 'fn'");
        assert!(
            keywords.contains(&"function"),
            "Should contain JS 'function'"
        );
        assert!(keywords.contains(&"def"), "Should contain Python 'def'");
        assert!(keywords.contains(&"func"), "Should contain Go 'func'");
    }

    #[test]
    fn test_all_iterator() {
        let langs: Vec<_> = SupportedLanguage::all().collect();
        assert_eq!(langs.len(), 7);
        assert!(langs.contains(&SupportedLanguage::Rust));
        assert!(langs.contains(&SupportedLanguage::TypeScript));
        assert!(langs.contains(&SupportedLanguage::TypeScriptReact));
        assert!(langs.contains(&SupportedLanguage::JavaScript));
        assert!(langs.contains(&SupportedLanguage::JavaScriptReact));
        assert!(langs.contains(&SupportedLanguage::Go));
        assert!(langs.contains(&SupportedLanguage::Python));
    }

    #[test]
    fn test_language_hash_eq() {
        use std::collections::HashMap;

        let mut map: HashMap<SupportedLanguage, &str> = HashMap::new();
        map.insert(SupportedLanguage::Rust, "rust");
        map.insert(SupportedLanguage::TypeScript, "typescript");

        assert_eq!(map.get(&SupportedLanguage::Rust), Some(&"rust"));
        assert_eq!(map.get(&SupportedLanguage::TypeScript), Some(&"typescript"));
        assert_eq!(map.get(&SupportedLanguage::Python), None);
    }
}
