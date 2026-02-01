//! Theme mapping for tree-sitter highlights.
//!
//! Maps tree-sitter capture names to ratatui styles.

use ratatui::style::{Color, Modifier, Style};

/// Map a tree-sitter capture name to a ratatui Style.
///
/// Capture names follow the convention from nvim-treesitter and other editors.
/// This provides a consistent color scheme across all supported languages.
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
}
