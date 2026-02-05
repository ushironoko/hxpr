//! Theme mapping for tree-sitter highlights.
//!
//! Maps tree-sitter capture names to ratatui styles using .tmTheme scope mapping.
//! This allows tree-sitter highlighting to respect the user's configured theme.

use std::collections::HashMap;
use std::str::FromStr;

use phf::phf_map;
use ratatui::style::{Color, Modifier, Style};
use syntect::highlighting::{FontStyle, Theme, ThemeItem};
use syntect::parsing::{MatchPower, ScopeStack};

/// Mapping from tree-sitter capture names to TextMate scope candidates.
///
/// Each capture name maps to a list of scope candidates, tried in order.
/// The first scope that has a style in the theme is used.
static CAPTURE_TO_SCOPES: phf::Map<&'static str, &'static [&'static str]> = phf_map! {
    // Keywords
    "keyword" => &["keyword.control", "keyword"],
    "keyword.function" => &["keyword.control", "storage.type.function", "keyword"],
    "keyword.return" => &["keyword.control.return", "keyword.control", "keyword"],
    "keyword.operator" => &["keyword.operator", "keyword"],
    "keyword.import" => &["keyword.control.import", "keyword"],
    "keyword.modifier" => &["storage.modifier", "keyword"],
    "keyword.control" => &["keyword.control", "keyword"],
    "keyword.conditional" => &["keyword.control.conditional", "keyword.control", "keyword"],
    "keyword.repeat" => &["keyword.control.loop", "keyword.control", "keyword"],
    "keyword.exception" => &["keyword.control.exception", "keyword.control", "keyword"],
    "keyword.coroutine" => &["keyword.control.flow", "keyword.control", "keyword"],
    "keyword.storage" => &["storage.modifier", "storage", "keyword"],

    // Functions
    "function" => &["entity.name.function", "support.function"],
    "function.call" => &["entity.name.function", "variable.function", "support.function"],
    "function.method" => &["entity.name.function.method", "entity.name.function"],
    "function.method.call" => &["entity.name.function.method", "entity.name.function", "support.function"],
    "function.macro" => &["entity.name.function.macro", "support.function"],
    "function.builtin" => &["support.function.builtin", "support.function"],

    // Types
    "type" => &["storage.type", "support.type", "entity.name.type"],
    "type.builtin" => &["storage.type.builtin", "support.type.builtin", "storage.type"],
    "type.definition" => &["entity.name.type", "storage.type"],
    "type.qualifier" => &["storage.modifier", "keyword.other"],

    // Strings & Literals
    "string" => &["string.quoted", "string"],
    "string.escape" => &["constant.character.escape"],
    "string.special" => &["string.regexp", "constant.other.placeholder", "string"],
    "string.regex" => &["string.regexp", "string"],
    "character" => &["constant.character", "string.quoted.single"],

    // Numbers & Constants
    "number" => &["constant.numeric", "constant.numeric.integer"],
    "number.float" => &["constant.numeric.float", "constant.numeric"],
    "boolean" => &["constant.language.boolean", "constant.language"],
    "constant" => &["constant", "constant.other"],
    "constant.builtin" => &["constant.language", "constant"],

    // Comments
    "comment" => &["comment", "comment.line", "comment.block"],
    "comment.line" => &["comment.line", "comment"],
    "comment.block" => &["comment.block", "comment"],
    "comment.documentation" => &["comment.block.documentation", "comment.block", "comment"],

    // Variables
    "variable" => &["variable", "variable.other"],
    "variable.parameter" => &["variable.parameter", "variable"],
    "variable.member" => &["variable.other.member", "variable"],

    // Properties & Fields
    "property" => &["variable.other.property", "entity.other.attribute-name"],
    "field" => &["variable.other.member", "variable.other.property"],
    "attribute" => &["entity.other.attribute-name", "meta.attribute"],

    // Operators & Punctuation
    "operator" => &["keyword.operator", "punctuation"],
    "punctuation" => &["punctuation"],
    "punctuation.bracket" => &["punctuation.section", "punctuation"],
    "punctuation.delimiter" => &["punctuation.separator", "punctuation"],
    "punctuation.special" => &["punctuation.definition", "punctuation"],

    // Other
    "label" => &["entity.name.label", "meta.label"],
    "tag" => &["entity.name.tag"],
    "namespace" => &["entity.name.namespace", "entity.name.module"],
    "module" => &["entity.name.module", "entity.name.namespace"],
    "constructor" => &["entity.name.function.constructor", "entity.name.class"],
    "escape" => &["constant.character.escape"],
    "include" => &["keyword.control.import", "keyword.other.import"],
    "embedded" => &["meta.embedded", "source"],
};

/// Cache of styles for each capture name, pre-computed from a theme.
///
/// This avoids repeated scope lookups during highlighting.
#[derive(Clone)]
pub struct ThemeStyleCache {
    cache: HashMap<&'static str, Style>,
}

impl ThemeStyleCache {
    /// Create a new style cache by pre-computing styles for all known capture names.
    ///
    /// This runs once when a new theme is loaded, so that highlighting is O(1).
    pub fn new(theme: &Theme) -> Self {
        let mut cache = HashMap::new();

        for (capture, scopes) in CAPTURE_TO_SCOPES.entries() {
            if let Some(style) = find_style_for_scopes(scopes, theme) {
                cache.insert(*capture, style);
            }
        }

        Self { cache }
    }

    /// Get the style for a capture name.
    ///
    /// Returns the cached style if available, otherwise falls back to hardcoded defaults.
    #[inline]
    pub fn get(&self, capture: &str) -> Style {
        self.cache
            .get(capture)
            .copied()
            .unwrap_or_else(|| style_for_capture(capture))
    }
}

/// Find the best style for a list of scope candidates in a theme.
///
/// Tries each scope in order and returns the first one that has a style defined.
fn find_style_for_scopes(scopes: &[&str], theme: &Theme) -> Option<Style> {
    for scope_str in scopes {
        if let Some(style) = find_style_for_scope(scope_str, theme) {
            return Some(style);
        }
    }
    None
}

/// Find the style for a single scope string in a theme.
fn find_style_for_scope(scope_str: &str, theme: &Theme) -> Option<Style> {
    let scope_stack = ScopeStack::from_str(scope_str).ok()?;

    // Find the best matching scope in the theme
    let mut best_match: Option<(MatchPower, &ThemeItem)> = None;

    for item in &theme.scopes {
        if let Some(match_power) = item.scope.does_match(scope_stack.as_slice()) {
            match &mut best_match {
                None => best_match = Some((match_power, item)),
                Some((best_power, _)) if match_power > *best_power => {
                    best_match = Some((match_power, item));
                }
                _ => {}
            }
        }
    }

    best_match.map(|(_, item)| convert_theme_style(&item.style, theme))
}

/// Convert a syntect StyleModifier to a ratatui Style.
fn convert_theme_style(style_mod: &syntect::highlighting::StyleModifier, theme: &Theme) -> Style {
    let mut style = Style::default();

    // Apply foreground color (fall back to theme's default foreground)
    if let Some(fg) = style_mod.foreground {
        style = style.fg(Color::Rgb(fg.r, fg.g, fg.b));
    } else if let Some(fg) = theme.settings.foreground {
        style = style.fg(Color::Rgb(fg.r, fg.g, fg.b));
    }

    // Background color is NOT applied - we preserve terminal background

    // Apply font style modifiers
    if let Some(font_style) = style_mod.font_style {
        if font_style.contains(FontStyle::BOLD) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if font_style.contains(FontStyle::ITALIC) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if font_style.contains(FontStyle::UNDERLINE) {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
    }

    style
}

/// Map a tree-sitter capture name to a ratatui Style (hardcoded fallback).
///
/// This provides default colors when no theme mapping is available.
/// Capture names follow the convention from nvim-treesitter and other editors.
pub fn style_for_capture(capture_name: &str) -> Style {
    match capture_name {
        // Keywords
        "keyword"
        | "keyword.function"
        | "keyword.control"
        | "keyword.return"
        | "keyword.conditional"
        | "keyword.repeat"
        | "keyword.operator"
        | "keyword.import"
        | "keyword.exception"
        | "keyword.coroutine" => Style::default().fg(Color::Magenta),

        "keyword.modifier" | "keyword.storage" => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::ITALIC),

        // Types
        "type" | "type.builtin" | "type.definition" | "type.qualifier" => {
            Style::default().fg(Color::Yellow)
        }

        // Functions and methods
        "function"
        | "function.call"
        | "function.method"
        | "function.method.call"
        | "function.builtin"
        | "function.macro" => Style::default().fg(Color::Blue),

        // Strings and literals
        "string" | "string.special" | "string.escape" | "string.regex" | "character" => {
            Style::default().fg(Color::Green)
        }

        // Numbers and constants
        "number" | "number.float" | "constant" | "constant.builtin" | "boolean" => {
            Style::default().fg(Color::Cyan)
        }

        // Comments
        "comment" | "comment.line" | "comment.block" | "comment.documentation" => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),

        // Variables and parameters
        "variable" | "variable.parameter" | "variable.member" => Style::default().fg(Color::White),

        // Properties and fields
        "property" | "field" | "attribute" => Style::default().fg(Color::LightBlue),

        // Operators and punctuation
        "operator" => Style::default().fg(Color::White),

        "punctuation" | "punctuation.bracket" | "punctuation.delimiter" | "punctuation.special" => {
            Style::default().fg(Color::Gray)
        }

        // Labels, tags, namespaces
        "label" | "tag" => Style::default().fg(Color::Red),
        "namespace" | "module" => Style::default().fg(Color::Yellow),

        // Special
        "escape" => Style::default().fg(Color::Cyan),
        "constructor" => Style::default().fg(Color::Yellow),
        "include" => Style::default().fg(Color::Magenta),

        // Embedded content (for interpolation, etc.)
        "embedded" => Style::default(),

        // Default: no special styling
        _ => Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::get_theme;

    #[test]
    fn test_keyword_styles() {
        let style = style_for_capture("keyword");
        assert_eq!(style.fg, Some(Color::Magenta));

        let style = style_for_capture("keyword.function");
        assert_eq!(style.fg, Some(Color::Magenta));
    }

    #[test]
    fn test_type_styles() {
        let style = style_for_capture("type");
        assert_eq!(style.fg, Some(Color::Yellow));

        let style = style_for_capture("type.builtin");
        assert_eq!(style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_function_styles() {
        let style = style_for_capture("function");
        assert_eq!(style.fg, Some(Color::Blue));

        let style = style_for_capture("function.call");
        assert_eq!(style.fg, Some(Color::Blue));
    }

    #[test]
    fn test_string_styles() {
        let style = style_for_capture("string");
        assert_eq!(style.fg, Some(Color::Green));
    }

    #[test]
    fn test_comment_styles() {
        let style = style_for_capture("comment");
        assert_eq!(style.fg, Some(Color::DarkGray));
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_unknown_capture() {
        let style = style_for_capture("unknown_capture_name");
        assert_eq!(style, Style::default());
    }

    #[test]
    fn test_theme_style_cache_dracula() {
        let theme = get_theme("Dracula");
        let cache = ThemeStyleCache::new(theme);

        // Keywords should have a color from Dracula theme
        let keyword_style = cache.get("keyword");
        assert!(
            keyword_style.fg.is_some(),
            "keyword should have foreground color from Dracula theme"
        );

        // Functions should have a color
        let func_style = cache.get("function");
        assert!(
            func_style.fg.is_some(),
            "function should have foreground color from Dracula theme"
        );

        // Strings should have a color
        let string_style = cache.get("string");
        assert!(
            string_style.fg.is_some(),
            "string should have foreground color from Dracula theme"
        );
    }

    #[test]
    fn test_theme_style_cache_actually_caches() {
        let theme = get_theme("Dracula");
        let cache = ThemeStyleCache::new(theme);

        // Check that the cache is not empty
        assert!(
            !cache.cache.is_empty(),
            "ThemeStyleCache should have cached entries"
        );

        // Verify that cached styles differ from hardcoded fallbacks for at least some captures
        let mut differs_count = 0;
        for (capture, cached_style) in &cache.cache {
            let fallback_style = style_for_capture(capture);
            if cached_style.fg != fallback_style.fg {
                differs_count += 1;
            }
        }

        assert!(
            differs_count > 0,
            "At least some cached styles should differ from fallback (theme should apply)"
        );
    }

    #[test]
    fn test_theme_style_cache_unknown_capture_fallback() {
        let theme = get_theme("Dracula");
        let cache = ThemeStyleCache::new(theme);

        // Unknown capture should fall back to hardcoded default
        let style = cache.get("unknown_capture_xyz");
        assert_eq!(style, Style::default());
    }

    #[test]
    fn test_find_style_for_scope() {
        let theme = get_theme("Dracula");

        // Basic scope should find a style
        let style = find_style_for_scope("keyword", theme);
        assert!(style.is_some(), "keyword scope should match in Dracula");

        let style = find_style_for_scope("string", theme);
        assert!(style.is_some(), "string scope should match in Dracula");
    }

    #[test]
    fn test_capture_to_scopes_coverage() {
        // Verify all hardcoded capture names in style_for_capture have a mapping
        let hardcoded_captures = [
            "keyword",
            "keyword.function",
            "keyword.control",
            "keyword.return",
            "keyword.conditional",
            "keyword.repeat",
            "keyword.operator",
            "keyword.import",
            "keyword.exception",
            "keyword.coroutine",
            "keyword.modifier",
            "keyword.storage",
            "type",
            "type.builtin",
            "type.definition",
            "type.qualifier",
            "function",
            "function.call",
            "function.method",
            "function.method.call",
            "function.builtin",
            "function.macro",
            "string",
            "string.special",
            "string.escape",
            "string.regex",
            "character",
            "number",
            "number.float",
            "constant",
            "constant.builtin",
            "boolean",
            "comment",
            "comment.line",
            "comment.block",
            "comment.documentation",
            "variable",
            "variable.parameter",
            "variable.member",
            "property",
            "field",
            "attribute",
            "operator",
            "punctuation",
            "punctuation.bracket",
            "punctuation.delimiter",
            "punctuation.special",
            "label",
            "tag",
            "namespace",
            "module",
            "escape",
            "constructor",
            "include",
            "embedded",
        ];

        for capture in hardcoded_captures {
            assert!(
                CAPTURE_TO_SCOPES.contains_key(capture),
                "CAPTURE_TO_SCOPES missing mapping for: {}",
                capture
            );
        }
    }
}
