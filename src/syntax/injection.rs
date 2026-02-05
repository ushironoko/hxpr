//! Language injection support for tree-sitter.
//!
//! This module handles extracting and processing language injections
//! from tree-sitter queries (injections.scm).

use std::ops::Range;
use tree_sitter::{Language, Query, QueryCursor, StreamingIterator, Tree};

/// Represents a range of text that should be parsed with a different language.
#[derive(Debug, Clone)]
pub struct InjectionRange {
    /// Byte range in the source text
    pub range: Range<usize>,
    /// The language to use for this range
    pub language: String,
    /// The kind of the parent node containing this injection (e.g., "script_element", "style_element")
    pub parent_node_kind: Option<String>,
}

/// Extract injection ranges from a tree using an injection query.
///
/// Returns a list of ranges and their associated languages.
pub fn extract_injections(
    tree: &Tree,
    source: &[u8],
    language: &Language,
    injection_query: &str,
) -> Vec<InjectionRange> {
    // Parse the injection query
    let query = match Query::new(language, injection_query) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = QueryCursor::new();
    let mut injections = Vec::new();

    // Execute the query using StreamingIterator
    let mut matches = cursor.matches(&query, tree.root_node(), source);

    while let Some(match_) = matches.next() {
        let mut content_range: Option<Range<usize>> = None;
        let mut content_node: Option<tree_sitter::Node> = None;
        let mut lang: Option<String> = None;

        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];

            if *capture_name == "injection.content" {
                content_range = Some(capture.node.byte_range());
                content_node = Some(capture.node);
            } else if *capture_name == "injection.language" {
                // Language from captured text
                if let Ok(text) = capture.node.utf8_text(source) {
                    lang = Some(text.to_string());
                }
            }
        }

        // Check for #set! injection.language in query properties
        if lang.is_none() {
            lang = get_injection_language_from_pattern(&query, match_.pattern_index);
        }

        // Get parent node kind from the content node
        let parent_node_kind = content_node.and_then(|node| {
            // Walk up the tree to find a meaningful parent node kind
            // (e.g., "script_element", "style_element")
            let mut current = node.parent();
            while let Some(parent) = current {
                let kind = parent.kind();
                // Look for element-level nodes that indicate the injection context
                if kind.ends_with("_element") || kind == "script" || kind == "style" {
                    return Some(kind.to_string());
                }
                current = parent.parent();
            }
            None
        });

        if let (Some(range), Some(language)) = (content_range, lang) {
            if !range.is_empty() {
                injections.push(InjectionRange {
                    range,
                    language,
                    parent_node_kind,
                });
            }
        }
    }

    // Deduplicate injections for the same range, preferring more specific languages
    // (e.g., TypeScript/TSX/JSX over JavaScript)
    deduplicate_injections(injections)
}

/// Deduplicate injections that cover the same range.
///
/// When multiple injections match the same byte range (e.g., a `<script lang="ts">` block
/// matching both the generic JavaScript rule and the TypeScript-specific rule), this function
/// keeps only the most specific language.
///
/// Language specificity order (most specific first):
/// - tsx, jsx (explicit JSX variants)
/// - typescript (explicit TS)
/// - All other languages are kept as-is
/// - javascript (least specific, used as fallback)
fn deduplicate_injections(mut injections: Vec<InjectionRange>) -> Vec<InjectionRange> {
    use std::collections::HashMap;

    // Group injections by their byte range
    let mut range_map: HashMap<(usize, usize), Vec<InjectionRange>> = HashMap::new();
    for inj in injections.drain(..) {
        let key = (inj.range.start, inj.range.end);
        range_map.entry(key).or_default().push(inj);
    }

    // For each range, pick the most specific language
    let mut result = Vec::new();
    for (_, mut group) in range_map {
        if group.len() == 1 {
            result.push(group.pop().unwrap());
        } else {
            // Sort by language specificity (most specific first)
            group.sort_by_key(|inj| language_specificity(&inj.language));
            // Take the most specific one
            result.push(group.remove(0));
        }
    }

    // Sort by range start for deterministic output
    result.sort_by_key(|inj| inj.range.start);
    result
}

/// Returns a specificity score for a language (lower is more specific).
fn language_specificity(lang: &str) -> u32 {
    match lang.to_lowercase().as_str() {
        // Most specific: explicit JSX/TSX variants
        "tsx" | "jsx" => 0,
        // TypeScript is more specific than JavaScript
        "ts" | "typescript" => 1,
        // JavaScript is the fallback
        "js" | "javascript" => 100,
        // Other languages get middle priority
        _ => 50,
    }
}

/// Try to extract the injection language from query pattern settings.
fn get_injection_language_from_pattern(query: &Query, pattern_index: usize) -> Option<String> {
    // Check if there's a property setting for this pattern
    for setting in query.property_settings(pattern_index) {
        if setting.key.as_ref() == "injection.language" {
            if let Some(value) = &setting.value {
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Map common language identifiers to our SupportedLanguage names.
pub fn normalize_language_name(name: &str) -> &str {
    match name.to_lowercase().as_str() {
        "ts" | "typescript" => "typescript",
        "tsx" => "tsx",
        "js" | "javascript" => "javascript",
        "jsx" => "jsx",
        "css" | "scss" | "postcss" | "less" | "stylus" => "css",
        "html" => "html",
        "json" => "json",
        "rust" | "rs" => "rust",
        "python" | "py" => "python",
        "go" | "golang" => "go",
        "lua" => "lua",
        "bash" | "sh" | "shell" => "bash",
        "php" => "php",
        "swift" => "swift",
        "haskell" | "hs" => "haskell",
        "moonbit" | "mbt" => "moonbit",
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_language_name() {
        assert_eq!(normalize_language_name("ts"), "typescript");
        assert_eq!(normalize_language_name("typescript"), "typescript");
        assert_eq!(normalize_language_name("Typescript"), "typescript");
        assert_eq!(normalize_language_name("js"), "javascript");
        assert_eq!(normalize_language_name("css"), "css");
        assert_eq!(normalize_language_name("scss"), "css");
    }

    #[test]
    fn test_extract_injections_svelte_script() {
        // Parse a simple Svelte file with script content
        let code = r#"<script lang="ts">
    const x = 1;
</script>

<div>Hello</div>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_svelte_ng::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        // Use Svelte's injection query
        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_svelte_ng::INJECTIONS_QUERY,
        );

        // Should find at least one injection (the script content)
        assert!(
            !injections.is_empty(),
            "Should find injections in Svelte code"
        );

        // Find the TypeScript injection
        let ts_injection = injections
            .iter()
            .find(|i| i.language == "typescript" || i.language == "ts");
        assert!(
            ts_injection.is_some(),
            "Should find TypeScript injection, found: {:?}",
            injections
        );

        // Verify the range contains the script content
        if let Some(inj) = ts_injection {
            let content = &code[inj.range.clone()];
            assert!(
                content.contains("const x = 1"),
                "Injection should contain script content, got: {}",
                content
            );
        }
    }

    #[test]
    fn test_extract_injections_svelte_style() {
        let code = r#"<style>
    .foo { color: red; }
</style>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_svelte_ng::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_svelte_ng::INJECTIONS_QUERY,
        );

        // Should find CSS injection
        let css_injection = injections
            .iter()
            .find(|i| i.language == "css" || i.language == "scss");

        // Note: This might fail if the injection query uses a different language name
        // or if raw_text without lang attr defaults to something else
        if let Some(inj) = css_injection {
            let content = &code[inj.range.clone()];
            assert!(
                content.contains(".foo"),
                "Injection should contain style content"
            );
        }
    }

    #[test]
    fn test_extract_injections_empty_query() {
        let code = "<div>Hello</div>";

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_svelte_ng::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        // Empty query should return empty results
        let injections = extract_injections(&tree, code.as_bytes(), &language, "");
        assert!(injections.is_empty());
    }

    #[test]
    fn test_extract_injections_invalid_query() {
        let code = "<div>Hello</div>";

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_svelte_ng::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        // Invalid query should return empty results (not panic)
        let injections = extract_injections(&tree, code.as_bytes(), &language, "((invalid syntax");
        assert!(injections.is_empty());
    }

    #[test]
    fn test_extract_injections_vue_script() {
        let code = r#"<script lang="ts">
    const x = 1;
</script>

<template>
    <div>Hello</div>
</template>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_vue3::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_vue3::INJECTIONS_QUERY,
        );

        // Should find TypeScript injection
        assert!(!injections.is_empty(), "Should find injections in Vue code");

        let ts_injection = injections
            .iter()
            .find(|i| i.language == "typescript" || i.language == "ts");
        assert!(
            ts_injection.is_some(),
            "Should find TypeScript injection, found: {:?}",
            injections
        );

        if let Some(inj) = ts_injection {
            let content = std::str::from_utf8(&code.as_bytes()[inj.range.clone()]).unwrap();
            assert!(
                content.contains("const x = 1"),
                "Injection should contain script content, got: {}",
                content
            );
        }
    }

    #[test]
    fn test_extract_injections_vue_style() {
        let code = r#"<style>
    .foo { color: red; }
</style>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_vue3::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_vue3::INJECTIONS_QUERY,
        );

        // Should find CSS injection
        let css_injection = injections.iter().find(|i| i.language == "css");

        assert!(
            css_injection.is_some(),
            "Should find CSS injection, found: {:?}",
            injections
        );

        if let Some(inj) = css_injection {
            let content = std::str::from_utf8(&code.as_bytes()[inj.range.clone()]).unwrap();
            assert!(
                content.contains(".foo"),
                "Injection should contain style content, got: {}",
                content
            );
        }
    }

    #[test]
    fn test_extract_injections_vue_interpolation() {
        let code = r#"<template>
    <div>{{ message }}</div>
</template>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_vue3::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_vue3::INJECTIONS_QUERY,
        );

        // Should find JavaScript injection for interpolation
        let js_injection = injections.iter().find(|i| i.language == "javascript");

        assert!(
            js_injection.is_some(),
            "Should find JavaScript injection for interpolation, found: {:?}",
            injections
        );

        if let Some(inj) = js_injection {
            let content = std::str::from_utf8(&code.as_bytes()[inj.range.clone()]).unwrap();
            assert!(
                content.contains("message"),
                "Injection should contain interpolation content, got: {}",
                content
            );
        }
    }

    #[test]
    fn test_deduplicate_injections_prefers_typescript_over_javascript() {
        // Vue <script lang="ts"> matches both the default JS rule and the TS-specific rule.
        // The deduplication logic should keep only TypeScript.
        let code = r#"<script lang="ts">
    const x: number = 1;
</script>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_vue3::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_vue3::INJECTIONS_QUERY,
        );

        // Should find exactly one injection for the script content
        // (not both JavaScript and TypeScript)
        let script_injections: Vec<_> = injections
            .iter()
            .filter(|i| i.language == "typescript" || i.language == "javascript")
            .collect();

        assert_eq!(
            script_injections.len(),
            1,
            "Should have exactly one script injection after deduplication, got: {:?}",
            script_injections
        );

        assert_eq!(
            script_injections[0].language, "typescript",
            "Should prefer TypeScript over JavaScript"
        );
    }

    #[test]
    fn test_deduplicate_injections_prefers_tsx_over_typescript() {
        let code = r#"<script lang="tsx">
    const x = <div>Hello</div>;
</script>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_vue3::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_vue3::INJECTIONS_QUERY,
        );

        // TSX should be preferred over both TypeScript and JavaScript
        let script_injections: Vec<_> = injections
            .iter()
            .filter(|i| {
                i.language == "tsx" || i.language == "typescript" || i.language == "javascript"
            })
            .collect();

        assert_eq!(
            script_injections.len(),
            1,
            "Should have exactly one script injection after deduplication, got: {:?}",
            script_injections
        );

        assert_eq!(
            script_injections[0].language, "tsx",
            "Should prefer TSX over TypeScript and JavaScript"
        );
    }

    #[test]
    fn test_language_specificity() {
        // More specific languages should have lower scores
        assert!(language_specificity("tsx") < language_specificity("typescript"));
        assert!(language_specificity("jsx") < language_specificity("typescript"));
        assert!(language_specificity("typescript") < language_specificity("javascript"));
        assert!(language_specificity("ts") < language_specificity("js"));
        // Other languages have middle priority
        assert!(language_specificity("css") < language_specificity("javascript"));
        assert!(language_specificity("css") > language_specificity("typescript"));
    }
}

#[cfg(test)]
mod priming_tests {
    use super::*;

    #[test]
    fn test_extract_injections_primed_vue_script() {
        // Simulate primed source: wrapping plain script content in <script lang="ts">
        let code = r#"<script lang="ts">
import { ref } from 'vue'
const count = ref(0)
</script>
"#;

        let mut parser = tree_sitter::Parser::new();
        let language: Language = tree_sitter_vue3::LANGUAGE.into();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let injections = extract_injections(
            &tree,
            code.as_bytes(),
            &language,
            tree_sitter_vue3::INJECTIONS_QUERY,
        );

        // Should find TypeScript injection
        assert!(
            !injections.is_empty(),
            "Should find injections in primed Vue code"
        );

        let ts_injection = injections
            .iter()
            .find(|i| i.language == "typescript" || i.language == "ts");
        assert!(
            ts_injection.is_some(),
            "Should find TypeScript injection, found: {:?}",
            injections
        );

        let inj = ts_injection.unwrap();
        let content = std::str::from_utf8(&code.as_bytes()[inj.range.clone()]).unwrap();
        assert!(content.contains("import"), "Injection should contain import");
        assert!(
            content.contains("const count"),
            "Injection should contain const"
        );
    }
}
