//! Syntax highlighting module using syntect.
//!
//! This module provides syntax highlighting for diff content using syntect
//! and converts the output to ratatui Span format using syntect-tui.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Get the global SyntaxSet instance.
/// This is lazily initialized on first access.
pub fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Get the global ThemeSet instance.
/// This is lazily initialized on first access.
pub fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Get the SyntaxReference for a file based on its extension.
///
/// # Arguments
/// * `filename` - The filename to get syntax for (e.g., "main.rs", "index.ts")
///
/// # Returns
/// * `Some(SyntaxReference)` - If a matching syntax was found
/// * `None` - If the extension is unknown or the file has no extension
pub fn syntax_for_file(filename: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())?;
    syntax_set().find_syntax_by_extension(ext)
}

/// Get a theme by name with fallback to default themes.
///
/// Falls back to "base16-ocean.dark" if the specified theme is not found,
/// and falls back to any available theme if that also fails.
///
/// # Arguments
/// * `name` - The name of the theme to get
///
/// # Returns
/// A reference to the theme
pub fn get_theme(name: &str) -> &'static syntect::highlighting::Theme {
    theme_set()
        .themes
        .get(name)
        .or_else(|| theme_set().themes.get("base16-ocean.dark"))
        .unwrap_or_else(|| {
            theme_set()
                .themes
                .values()
                .next()
                .expect("syntect default themes should never be empty")
        })
}

/// Highlight a code line and return a vector of owned Spans.
///
/// # Arguments
/// * `code` - The code line to highlight
/// * `highlighter` - A mutable reference to the HighlightLines instance
///
/// # Returns
/// A vector of `Span<'static>` with syntax highlighting applied.
/// If highlighting fails, returns plain text with no styling.
pub fn highlight_code_line(code: &str, highlighter: &mut HighlightLines<'_>) -> Vec<Span<'static>> {
    match highlighter.highlight_line(code, syntax_set()) {
        Ok(ranges) => ranges
            .into_iter()
            .map(|(style, text)| {
                // Convert syntect style to ratatui style, owning the string
                // We directly convert instead of using into_span to ensure 'static lifetime
                Span::styled(text.to_string(), convert_syntect_style(&style))
            })
            .collect(),
        Err(e) => {
            #[cfg(debug_assertions)]
            eprintln!("Highlight error: {e:?}");
            vec![Span::raw(code.to_string())]
        }
    }
}

/// Convert syntect Style to ratatui Style.
///
/// Note: Background color is intentionally NOT applied. Syntect themes define
/// a background color for the entire editor, but in a TUI diff viewer, we want
/// to preserve the terminal's background color for better visual consistency.
fn convert_syntect_style(style: &syntect::highlighting::Style) -> Style {
    let mut ratatui_style = Style::default();

    // Convert foreground color
    if style.foreground.a > 0 {
        ratatui_style = ratatui_style.fg(Color::Rgb(
            style.foreground.r,
            style.foreground.g,
            style.foreground.b,
        ));
    }

    // Background color is NOT applied - we want to keep the terminal's background
    // The theme's background is meant for the whole editor, not per-token

    // Convert font style
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }

    ratatui_style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_for_file_known_extension() {
        // Common extensions that should be in syntect defaults
        assert!(syntax_for_file("main.rs").is_some());
        assert!(syntax_for_file("script.py").is_some());
        assert!(syntax_for_file("main.go").is_some());
        assert!(syntax_for_file("index.js").is_some());
        assert!(syntax_for_file("style.css").is_some());
        assert!(syntax_for_file("index.html").is_some());
        // Test with path-like filenames (as returned by GitHub API)
        assert!(syntax_for_file("src/app.rs").is_some(), "src/app.rs should have syntax");
        assert!(syntax_for_file("src/ui/diff_view.rs").is_some());
    }

    #[test]
    fn test_syntax_for_file_unknown_extension() {
        assert!(syntax_for_file("file.unknown_ext_xyz").is_none());
    }

    #[test]
    fn test_syntax_for_file_no_extension() {
        assert!(syntax_for_file("Makefile").is_none());
        assert!(syntax_for_file("README").is_none());
    }

    #[test]
    fn test_get_theme_existing() {
        let theme = get_theme("base16-ocean.dark");
        // Should not panic
        assert!(!theme.scopes.is_empty() || theme.settings.background.is_some());
    }

    #[test]
    fn test_get_theme_fallback() {
        // Non-existent theme should fall back without panic
        let theme = get_theme("non_existent_theme_xyz");
        assert!(!theme.scopes.is_empty() || theme.settings.background.is_some());
    }

    #[test]
    fn test_highlight_code_line_rust() {
        let syntax = syntax_for_file("test.rs").unwrap();
        let theme = get_theme("base16-ocean.dark");
        let mut highlighter = HighlightLines::new(syntax, theme);

        let spans = highlight_code_line("let app = Self {", &mut highlighter);
        assert!(!spans.is_empty());

        // Verify that "let" keyword has a foreground color (syntax highlighting applied)
        let let_span = spans.iter().find(|s| s.content.as_ref() == "let");
        assert!(let_span.is_some(), "Should have a span for 'let'");
        let let_style = let_span.unwrap().style;
        assert!(let_style.fg.is_some(), "'let' should have foreground color");

        // Verify that background color is NOT applied (we preserve terminal background)
        assert!(let_style.bg.is_none(), "'let' should NOT have background color");
    }

    #[test]
    fn test_highlight_code_line_empty() {
        let syntax = syntax_for_file("test.rs").unwrap();
        let theme = get_theme("base16-ocean.dark");
        let mut highlighter = HighlightLines::new(syntax, theme);

        let spans = highlight_code_line("", &mut highlighter);
        // Empty line should produce empty or single empty span
        assert!(spans.is_empty() || (spans.len() == 1 && spans[0].content.is_empty()));
    }
}
