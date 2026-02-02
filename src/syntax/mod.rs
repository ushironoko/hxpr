//! Syntax highlighting module using tree-sitter and syntect.
//!
//! This module provides syntax highlighting for diff content using:
//! - **tree-sitter**: For supported languages (Rust, TypeScript, JavaScript, Go, Python)
//! - **syntect**: As a fallback for other languages (Vue, YAML, Markdown, etc.)
//!
//! ## Supported Languages (tree-sitter)
//!
//! - Rust (.rs)
//! - TypeScript (.ts, .tsx)
//! - JavaScript (.js, .jsx)
//! - Go (.go)
//! - Python (.py)
//!
//! ## Fallback Languages (syntect via two-face)
//!
//! - Vue (.vue)
//! - YAML (.yaml, .yml)
//! - TOML (.toml)
//! - Markdown (.md)
//! - SCSS (.scss)
//! - Svelte (.svelte)
//! - And many more...
//!
//! ## Theme Loading
//!
//! Themes are loaded from two sources:
//! 1. **Bundled themes**: two-face extras + Dracula (compiled into binary)
//! 2. **User themes**: `~/.config/octorus/themes/*.tmTheme` files
//!
//! User themes override bundled themes if they have the same name.

pub mod highlighter;
pub mod parser_pool;
pub mod themes;

pub use highlighter::{
    apply_line_highlights, collect_line_highlights, CstParseResult, Highlighter, LineHighlights,
};
pub use parser_pool::ParserPool;
pub use themes::ThemeStyleCache;

use std::io::Cursor;
use std::sync::OnceLock;

use lasso::Rodeo;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use xdg::BaseDirectories;

use crate::app::InternedSpan;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Bundled Dracula theme (compiled into binary)
const DRACULA_THEME: &[u8] = include_bytes!("../../themes/Dracula.tmTheme");

/// Get the global SyntaxSet instance.
/// This is lazily initialized on first access.
/// Uses two-face's extended syntax definitions for broader language support.
pub fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

/// Get the global ThemeSet instance.
/// This is lazily initialized on first access.
///
/// Loads themes in the following order:
/// 1. Syntect default themes
/// 2. Bundled themes (Dracula)
/// 3. User themes from ~/.config/octorus/themes/
pub fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(load_all_themes)
}

/// Load all themes from syntect defaults, bundled, and user sources.
///
/// Note: We use syntect's ThemeSet for flexibility (custom themes, string-based lookup).
/// two-face's EmbeddedLazyThemeSet is more limited but we benefit from its extended syntax set.
fn load_all_themes() -> ThemeSet {
    let mut themes = ThemeSet::load_defaults();

    // Load bundled Dracula theme
    if let Ok(theme) = ThemeSet::load_from_reader(&mut Cursor::new(DRACULA_THEME)) {
        themes.themes.insert("Dracula".to_string(), theme);
    }

    // Load user themes from ~/.config/octorus/themes/
    if let Ok(base_dirs) = BaseDirectories::with_prefix("octorus") {
        let user_themes_dir = base_dirs.get_config_home().join("themes");
        if user_themes_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&user_themes_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "tmTheme") {
                        if let Ok(theme) = ThemeSet::get_theme(&path) {
                            // Use filename without extension as theme name
                            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                                themes.themes.insert(name.to_string(), theme);
                            }
                        }
                    }
                }
            }
        }
    }

    themes
}

/// List all available theme names.
pub fn available_themes() -> Vec<&'static str> {
    theme_set().themes.keys().map(|s| s.as_str()).collect()
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
/// Theme matching is case-insensitive. Falls back to "base16-ocean.dark"
/// if the specified theme is not found, and falls back to any available
/// theme if that also fails.
///
/// # Arguments
/// * `name` - The name of the theme to get
///
/// # Returns
/// A reference to the theme
pub fn get_theme(name: &str) -> &'static syntect::highlighting::Theme {
    let themes = &theme_set().themes;

    // Try exact match first
    if let Some(theme) = themes.get(name) {
        return theme;
    }

    // Try case-insensitive match
    let name_lower = name.to_lowercase();
    for (key, theme) in themes.iter() {
        if key.to_lowercase() == name_lower {
            return theme;
        }
    }

    // Fallback to default themes
    themes
        .get("base16-ocean.dark")
        .or_else(|| themes.values().next())
        .expect("syntect default themes should never be empty")
}

/// Highlight a code line and return a vector of InternedSpans.
///
/// # Arguments
/// * `code` - The code line to highlight
/// * `highlighter` - A mutable reference to the HighlightLines instance
/// * `interner` - A mutable reference to the string interner
///
/// # Returns
/// A vector of `InternedSpan` with syntax highlighting applied.
/// If highlighting fails, returns plain text with no styling.
pub fn highlight_code_line(
    code: &str,
    highlighter: &mut HighlightLines<'_>,
    interner: &mut Rodeo,
) -> Vec<InternedSpan> {
    match highlighter.highlight_line(code, syntax_set()) {
        Ok(ranges) => ranges
            .into_iter()
            .map(|(style, text)| {
                // Intern the text to avoid allocations for duplicate tokens
                InternedSpan {
                    content: interner.get_or_intern(text),
                    style: convert_syntect_style(&style),
                }
            })
            .collect(),
        Err(_e) => {
            #[cfg(debug_assertions)]
            eprintln!("Highlight error: {_e:?}");
            vec![InternedSpan {
                content: interner.get_or_intern(code),
                style: Style::default(),
            }]
        }
    }
}

/// Highlight a code line and return a vector of owned Spans (legacy API).
///
/// This function is kept for backward compatibility with tests.
/// For production code, prefer `highlight_code_line` with interner.
pub fn highlight_code_line_legacy(
    code: &str,
    highlighter: &mut HighlightLines<'_>,
) -> Vec<Span<'static>> {
    match highlighter.highlight_line(code, syntax_set()) {
        Ok(ranges) => ranges
            .into_iter()
            .map(|(style, text)| Span::styled(text.to_string(), convert_syntect_style(&style)))
            .collect(),
        Err(_e) => {
            #[cfg(debug_assertions)]
            eprintln!("Highlight error: {_e:?}");
            vec![Span::raw(code.to_string())]
        }
    }
}

/// Convert syntect Style to ratatui Style.
///
/// Note: Background color is intentionally NOT applied. Syntect themes define
/// a background color for the entire editor, but in a TUI diff viewer, we want
/// to preserve the terminal's background color for better visual consistency.
pub fn convert_syntect_style(style: &syntect::highlighting::Style) -> Style {
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
        // Common extensions from syntect defaults
        assert!(syntax_for_file("main.rs").is_some());
        assert!(syntax_for_file("script.py").is_some());
        assert!(syntax_for_file("main.go").is_some());
        assert!(syntax_for_file("index.js").is_some());
        assert!(syntax_for_file("style.css").is_some());
        assert!(syntax_for_file("index.html").is_some());

        // Extensions added by two-face (bat syntax definitions)
        assert!(
            syntax_for_file("index.ts").is_some(),
            "TypeScript should be supported"
        );
        assert!(
            syntax_for_file("app.tsx").is_some(),
            "TSX should be supported"
        );
        assert!(
            syntax_for_file("app.vue").is_some(),
            "Vue should be supported"
        );
        assert!(
            syntax_for_file("config.toml").is_some(),
            "TOML should be supported"
        );
        assert!(
            syntax_for_file("style.scss").is_some(),
            "SCSS should be supported"
        );
        assert!(
            syntax_for_file("App.svelte").is_some(),
            "Svelte should be supported"
        );
        // Note: .jsx is NOT supported by two-face/bat, use .tsx instead

        // Test with path-like filenames (as returned by GitHub API)
        assert!(
            syntax_for_file("src/app.rs").is_some(),
            "src/app.rs should have syntax"
        );
        assert!(syntax_for_file("src/ui/diff_view.rs").is_some());
        assert!(syntax_for_file("src/components/Button.vue").is_some());
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

        let spans = highlight_code_line_legacy("let app = Self {", &mut highlighter);
        assert!(!spans.is_empty());

        // Verify that "let" keyword has a foreground color (syntax highlighting applied)
        let let_span = spans.iter().find(|s| s.content.as_ref() == "let");
        assert!(let_span.is_some(), "Should have a span for 'let'");
        let let_style = let_span.unwrap().style;
        assert!(let_style.fg.is_some(), "'let' should have foreground color");

        // Verify that background color is NOT applied (we preserve terminal background)
        assert!(
            let_style.bg.is_none(),
            "'let' should NOT have background color"
        );
    }

    #[test]
    fn test_highlight_code_line_empty() {
        let syntax = syntax_for_file("test.rs").unwrap();
        let theme = get_theme("base16-ocean.dark");
        let mut highlighter = HighlightLines::new(syntax, theme);

        let spans = highlight_code_line_legacy("", &mut highlighter);
        // Empty line should produce empty or single empty span
        assert!(spans.is_empty() || (spans.len() == 1 && spans[0].content.is_empty()));
    }

    #[test]
    fn test_bundled_dracula_theme() {
        // Dracula theme should be available from bundled themes
        let theme = get_theme("Dracula");
        assert!(!theme.scopes.is_empty(), "Dracula should have scopes");
    }

    #[test]
    fn test_available_themes_includes_defaults_and_bundled() {
        let themes = available_themes();
        // Should include syntect defaults
        assert!(
            themes.contains(&"base16-ocean.dark"),
            "Should include base16-ocean.dark"
        );
        // Should include bundled Dracula
        assert!(themes.contains(&"Dracula"), "Should include Dracula");
    }

    #[test]
    fn test_highlight_with_dracula() {
        let syntax = syntax_for_file("test.rs").unwrap();
        let theme = get_theme("Dracula");
        let mut highlighter = HighlightLines::new(syntax, theme);

        let spans = highlight_code_line_legacy("fn main() {", &mut highlighter);
        assert!(!spans.is_empty());

        // fn keyword should be highlighted
        let fn_span = spans.iter().find(|s| s.content.as_ref() == "fn");
        assert!(fn_span.is_some(), "Should have a span for 'fn'");
        assert!(
            fn_span.unwrap().style.fg.is_some(),
            "'fn' should have foreground color"
        );
    }

    #[test]
    fn test_get_theme_case_insensitive() {
        // Theme names should match case-insensitively
        let theme1 = get_theme("Dracula");
        let theme2 = get_theme("dracula");
        let theme3 = get_theme("DRACULA");

        // All should return the same Dracula theme (not fallback)
        assert!(!theme1.scopes.is_empty());
        assert!(!theme2.scopes.is_empty());
        assert!(!theme3.scopes.is_empty());

        // Verify they have the same number of scopes (same theme)
        assert_eq!(theme1.scopes.len(), theme2.scopes.len());
        assert_eq!(theme1.scopes.len(), theme3.scopes.len());
    }

    #[test]
    fn test_highlight_code_line_typescript() {
        let syntax = syntax_for_file("test.ts").unwrap();
        let theme = get_theme("base16-ocean.dark");
        let mut highlighter = HighlightLines::new(syntax, theme);

        let spans = highlight_code_line_legacy("const foo: string = 'bar';", &mut highlighter);
        assert!(!spans.is_empty());

        // Verify that "const" keyword has a foreground color (syntax highlighting applied)
        let const_span = spans.iter().find(|s| s.content.as_ref() == "const");
        assert!(const_span.is_some(), "Should have a span for 'const'");
        assert!(
            const_span.unwrap().style.fg.is_some(),
            "'const' should have foreground color"
        );
    }

    #[test]
    fn test_highlight_code_line_vue() {
        let syntax = syntax_for_file("test.vue").unwrap();
        let theme = get_theme("Dracula");
        let mut highlighter = HighlightLines::new(syntax, theme);

        let spans = highlight_code_line_legacy("<template>", &mut highlighter);
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_code_line_with_interner() {
        let syntax = syntax_for_file("test.rs").unwrap();
        let theme = get_theme("base16-ocean.dark");
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut interner = Rodeo::default();

        let spans = highlight_code_line("let app = Self {", &mut highlighter, &mut interner);
        assert!(!spans.is_empty());

        // Verify that the interner contains the expected tokens
        for span in &spans {
            let text = interner.resolve(&span.content);
            assert!(!text.is_empty() || spans.len() == 1);
        }
    }

    #[test]
    fn test_interner_deduplication() {
        let syntax = syntax_for_file("test.rs").unwrap();
        let theme = get_theme("base16-ocean.dark");
        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut interner = Rodeo::default();

        // Highlight two lines with the same keyword
        let spans1 = highlight_code_line("let x = 1;", &mut highlighter, &mut interner);
        let spans2 = highlight_code_line("let y = 2;", &mut highlighter, &mut interner);

        // Find "let" in both spans - they should have the same Spur
        let let_spur1 = spans1
            .iter()
            .find(|s| interner.resolve(&s.content) == "let")
            .map(|s| s.content);
        let let_spur2 = spans2
            .iter()
            .find(|s| interner.resolve(&s.content) == "let")
            .map(|s| s.content);

        assert!(let_spur1.is_some(), "First line should contain 'let'");
        assert!(let_spur2.is_some(), "Second line should contain 'let'");
        assert_eq!(
            let_spur1, let_spur2,
            "Both 'let' tokens should have the same Spur (interned)"
        );
    }
}
