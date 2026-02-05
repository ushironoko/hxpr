//! This crate provides Vue 3 language support for the [tree-sitter] parsing library.
//!
//! Typically, you will use the [`LANGUAGE`] constant to add this language to a
//! tree-sitter [`Parser`], and then use the parser to parse some code:
//!
//! ```
//! let code = r#"
//! <template>
//!     <div>{{ message }}</div>
//! </template>
//!
//! <script setup lang="ts">
//! const message = "Hello, Vue!"
//! </script>
//! "#;
//! let mut parser = tree_sitter::Parser::new();
//! let language = tree_sitter_vue3::LANGUAGE;
//! parser
//!     .set_language(&language.into())
//!     .expect("Error loading Vue 3 parser");
//! let tree = parser.parse(code, None).unwrap();
//! assert!(!tree.root_node().has_error());
//! ```
//!
//! [`Parser`]: https://docs.rs/tree-sitter/latest/tree_sitter/struct.Parser.html
//! [tree-sitter]: https://tree-sitter.github.io/

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_vue3() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_vue3) };

/// The content of the [`node-types.json`] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers/6-static-node-types
pub const NODE_TYPES: &str = include_str!("node-types.json");

/// The syntax highlighting query for this grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../queries/highlights.scm");

/// The injection query for this grammar (for embedding JavaScript/CSS in Vue SFC).
pub const INJECTIONS_QUERY: &str = include_str!("../queries/injections.scm");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&LANGUAGE.into())
            .expect("Error loading Vue 3 parser");
    }

    #[test]
    fn test_can_parse_code() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&LANGUAGE.into())
            .expect("Error loading Vue 3 parser");

        let code = r#"
<template>
    <div>{{ message }}</div>
</template>

<script setup lang="ts">
const message = "Hello, Vue!"
</script>
"#;
        let tree = parser.parse(code, None).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_can_parse_script_only() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&LANGUAGE.into())
            .expect("Error loading Vue 3 parser");

        let code = r#"
<script>
export default {
    data() {
        return { count: 0 }
    }
}
</script>
"#;
        let tree = parser.parse(code, None).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_highlights_query_not_empty() {
        assert!(!HIGHLIGHTS_QUERY.is_empty());
    }

    #[test]
    fn test_injections_query_not_empty() {
        assert!(!INJECTIONS_QUERY.is_empty());
    }
}
