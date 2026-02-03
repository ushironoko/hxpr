//! Symbol extraction and definition search for Go to Definition.
//!
//! Pure functions for extracting identifiers from source lines
//! and searching for definitions within diff patches and repositories.

use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;
use tokio::process::Command;

use crate::diff::{classify_line, LineType};
use crate::github::ChangedFile;
use crate::language::SupportedLanguage;

/// Grep pattern for definition search across repository.
///
/// Dynamically generated from `SupportedLanguage::all_definition_prefixes()`.
fn grep_definition_pattern() -> &'static str {
    static PATTERN: OnceLock<String> = OnceLock::new();
    PATTERN.get_or_init(|| {
        let prefixes = SupportedLanguage::all_definition_prefixes();
        let escaped: Vec<String> = prefixes
            .iter()
            .map(|p| {
                // Escape regex special characters
                p.chars()
                    .map(|c| match c {
                        '(' | ')' | '<' | '>' | '[' | ']' | '{' | '}' | '|' | '\\' | '.' | '*'
                        | '+' | '?' | '^' | '$' => {
                            let mut s = String::with_capacity(2);
                            s.push('\\');
                            s.push(c);
                            s
                        }
                        _ => c.to_string(),
                    })
                    .collect::<String>()
            })
            .collect();
        format!("({})", escaped.join("|"))
    })
}

/// Directories excluded from grep search.
const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
];

/// Check if a character is a valid identifier character.
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Check if a character is a valid identifier start character.
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Extract the word (identifier) at the given column position.
///
/// Returns `(word, start_column, end_column)` where the range is `[start, end)`.
/// Returns `None` if the column is not on an identifier character.
pub fn extract_word_at(content: &str, column: usize) -> Option<(&str, usize, usize)> {
    let chars: Vec<char> = content.chars().collect();

    if column >= chars.len() {
        return None;
    }

    if !is_ident_char(chars[column]) {
        return None;
    }

    // Find start of identifier
    let mut start = column;
    while start > 0 && is_ident_char(chars[start - 1]) {
        start -= 1;
    }

    // Ensure the identifier starts with a valid start character
    if !is_ident_start(chars[start]) {
        return None;
    }

    // Find end of identifier
    let mut end = column + 1;
    while end < chars.len() && is_ident_char(chars[end]) {
        end += 1;
    }

    // Convert char indices to byte offsets for slicing
    let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
    let byte_end: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();

    Some((&content[byte_start..byte_end], start, end))
}

/// Extract all unique identifiers from a line of code.
///
/// Returns a deduplicated list of `(word, start, end)` in order of first occurrence.
/// Skips common language keywords that are unlikely to be jump targets.
pub fn extract_all_identifiers(content: &str) -> Vec<(String, usize, usize)> {
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut result: Vec<(String, usize, usize)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut i = 0;

    while i < len {
        if is_ident_start(chars[i]) {
            let start = i;
            i += 1;
            while i < len && is_ident_char(chars[i]) {
                i += 1;
            }
            let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
            let byte_end: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
            let word = &content[byte_start..byte_end];

            if !is_common_keyword(word) && seen.insert(word.to_string()) {
                result.push((word.to_string(), start, i));
            }
        } else {
            i += 1;
        }
    }

    result
}

/// Common keywords that should be excluded from symbol popup candidates.
fn is_common_keyword(word: &str) -> bool {
    SupportedLanguage::all_keywords().contains(&word)
}

/// Check if a line imports the given symbol via `use` (Rust) or `import` (JS/TS/Python).
///
/// Matches patterns like:
/// - `use std::process::Command;`
/// - `use crate::foo::{Bar, Baz};`
/// - `import { Foo } from './module';`
/// - `import Foo from './module';`
/// - `from module import Foo`
pub fn is_import_line(content: &str, symbol: &str) -> bool {
    let trimmed = content.trim_start();

    // Rust: use ...::Symbol; or use ...::Symbol as ...;
    if let Some(use_body) = trimmed.strip_prefix("use ") {
        // Check grouped imports: use path::{..., Symbol, ...};
        if let Some(brace_start) = use_body.find('{') {
            if let Some(brace_end) = use_body.find('}') {
                let inside = &use_body[brace_start + 1..brace_end];
                for item in inside.split(',') {
                    let item = item.trim();
                    // Handle "Symbol" or "Symbol as Alias"
                    let name = item.split_whitespace().next().unwrap_or("");
                    if name == symbol {
                        return true;
                    }
                }
            }
        }

        // Check simple import: use path::Symbol; or use path::Symbol as Alias;
        let without_semi = use_body.trim_end_matches(';').trim();
        if let Some(last_colon) = without_semi.rfind("::") {
            let after_colon = &without_semi[last_colon + 2..];
            let name = after_colon.split_whitespace().next().unwrap_or("");
            if name == symbol {
                return true;
            }
        }
    }

    // JS/TS: import { Symbol } from '...'; or import Symbol from '...';
    if let Some(import_body) = trimmed.strip_prefix("import ") {
        // Named import: import { Foo, Bar } from ...
        if let Some(brace_start) = import_body.find('{') {
            if let Some(brace_end) = import_body.find('}') {
                let inside = &import_body[brace_start + 1..brace_end];
                for item in inside.split(',') {
                    let item = item.trim();
                    let name = item.split_whitespace().next().unwrap_or("");
                    if name == symbol {
                        return true;
                    }
                }
            }
        }

        // Default import: import Symbol from '...'
        let name = import_body.split_whitespace().next().unwrap_or("");
        if name == symbol {
            return true;
        }
    }

    // Python: from module import Symbol
    if trimmed.starts_with("from ") {
        if let Some(import_pos) = trimmed.find(" import ") {
            let after_import = &trimmed[import_pos + 8..];
            for item in after_import.split(',') {
                let item = item.trim();
                let name = item.split_whitespace().next().unwrap_or("");
                if name == symbol {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if a line is a definition of the given symbol.
///
/// Looks for known definition keyword prefixes followed by the symbol name
/// at a word boundary.
pub fn is_definition_line(content: &str, symbol: &str) -> bool {
    let trimmed = content.trim_start();

    for prefix in SupportedLanguage::all_definition_prefixes() {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            // Check if rest starts with the symbol
            if let Some(after_symbol) = rest.strip_prefix(symbol) {
                // Verify word boundary: next char must not be an ident char
                if after_symbol.is_empty()
                    || !is_ident_char(after_symbol.chars().next().unwrap_or(' '))
                {
                    return true;
                }
            }
        }
    }

    // Special case: `impl<...> Symbol` pattern for Rust generics
    if let Some(rest) = trimmed.strip_prefix("impl<") {
        // Find the closing '>'
        if let Some(pos) = rest.find('>') {
            let after_generic = rest[pos + 1..].trim_start();
            if let Some(after_symbol) = after_generic.strip_prefix(symbol) {
                if after_symbol.is_empty()
                    || !is_ident_char(after_symbol.chars().next().unwrap_or(' '))
                {
                    return true;
                }
            }
        }
    }

    false
}

/// Search for a symbol definition within the PR diff patches.
///
/// Returns `(file_index, diff_line_index)` if found.
/// Search order: current file first, then other files.
/// Two-pass strategy: first search for actual definitions (fn, struct, etc.),
/// then fall back to import/use statements.
pub fn find_definition_in_patches(
    symbol: &str,
    files: &[ChangedFile],
    current_file_index: usize,
) -> Option<(usize, usize)> {
    // Build search order: current file first, then others
    let mut search_order: Vec<usize> = Vec::with_capacity(files.len());
    if current_file_index < files.len() {
        search_order.push(current_file_index);
    }
    for i in 0..files.len() {
        if i != current_file_index {
            search_order.push(i);
        }
    }

    // Pass 1: Search for actual definitions (fn, struct, class, etc.)
    for &file_idx in &search_order {
        let file = &files[file_idx];
        let Some(ref patch) = file.patch else {
            continue;
        };

        for (line_idx, line) in patch.lines().enumerate() {
            let (line_type, content) = classify_line(line);

            // Only search in Added and Context lines
            if !matches!(line_type, LineType::Added | LineType::Context) {
                continue;
            }

            if is_definition_line(content, symbol) {
                return Some((file_idx, line_idx));
            }
        }
    }

    // Pass 2: Fall back to import/use statements
    for &file_idx in &search_order {
        let file = &files[file_idx];
        let Some(ref patch) = file.patch else {
            continue;
        };

        for (line_idx, line) in patch.lines().enumerate() {
            let (line_type, content) = classify_line(line);

            if !matches!(line_type, LineType::Added | LineType::Context) {
                continue;
            }

            if is_import_line(content, symbol) {
                return Some((file_idx, line_idx));
            }
        }
    }

    None
}

/// Search for a symbol definition in the local repository using `grep`.
///
/// Returns `(file_path, line_number)` if found (1-based line number).
/// Two-pass strategy: first search for definitions, then fall back to imports.
pub async fn find_definition_in_repo(
    symbol: &str,
    repo_root: &Path,
) -> Result<Option<(String, usize)>> {
    let pattern = format!(
        "{}{}",
        grep_definition_pattern(),
        regex_escape(symbol, true)
    );

    let mut cmd = Command::new("grep");
    cmd.arg("-rnE").arg(&pattern);

    // Add exclusion directories
    for dir in EXCLUDED_DIRS {
        cmd.arg(format!("--exclude-dir={}", dir));
    }

    cmd.arg(".").current_dir(repo_root);

    let output = cmd.output().await?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse matching lines: ./path/to/file:123:line content
        for result_line in stdout.lines() {
            let parts: Vec<&str> = result_line.splitn(3, ':').collect();
            if parts.len() >= 3 {
                let file_path = parts[0].strip_prefix("./").unwrap_or(parts[0]);
                if let Ok(line_number) = parts[1].parse::<usize>() {
                    if is_definition_line(parts[2], symbol) {
                        return Ok(Some((file_path.to_string(), line_number)));
                    }
                }
            }
        }
    }

    // Fall back to import/use statement search
    let import_pattern = format!(
        r"(use .*\b{sym}\b|import .*\b{sym}\b|from .* import .*\b{sym}\b)",
        sym = regex_escape(symbol, false)
    );

    let mut cmd = Command::new("grep");
    cmd.arg("-rnE").arg(&import_pattern);

    for dir in EXCLUDED_DIRS {
        cmd.arg(format!("--exclude-dir={}", dir));
    }

    cmd.arg(".").current_dir(repo_root);

    let output = cmd.output().await?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for result_line in stdout.lines() {
        let parts: Vec<&str> = result_line.splitn(3, ':').collect();
        if parts.len() >= 3 {
            let file_path = parts[0].strip_prefix("./").unwrap_or(parts[0]);
            if let Ok(line_number) = parts[1].parse::<usize>() {
                if is_import_line(parts[2], symbol) {
                    return Ok(Some((file_path.to_string(), line_number)));
                }
            }
        }
    }

    Ok(None)
}

/// Simple regex escape for symbol names (alphanumeric + underscore).
///
/// - `add_word_boundary`: if true, appends `[^a-zA-Z0-9_]` assertion for word boundary after symbol.
///   Set to false when the pattern already has its own boundary logic (e.g., `\b`).
fn regex_escape(s: &str, add_word_boundary: bool) -> String {
    let extra_cap = if add_word_boundary { 16 } else { 0 };
    let mut escaped = String::with_capacity(s.len() + extra_cap);
    for c in s.chars() {
        if is_ident_char(c) {
            escaped.push(c);
        } else {
            escaped.push('\\');
            escaped.push(c);
        }
    }
    if add_word_boundary {
        escaped.push_str("[^a-zA-Z0-9_]");
    }
    escaped
}

/// Calculate the next word boundary position (for `w` key).
///
/// Vim-compatible word motion: words are sequences of `[a-zA-Z0-9_]`
/// or sequences of non-whitespace, non-word characters.
/// Whitespace separates words, and transitions between word/non-word chars
/// are also boundaries.
pub fn next_word_boundary(content: &str, column: usize) -> usize {
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();

    if len == 0 || column >= len.saturating_sub(1) {
        return len.saturating_sub(1);
    }

    let mut pos = column;

    // Determine current character class
    let is_word = is_ident_char(chars[pos]);
    let is_space = chars[pos].is_whitespace();

    if is_space {
        // Skip whitespace, then return start of next word
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
        return pos.min(len.saturating_sub(1));
    }

    if is_word {
        // Skip word chars
        while pos < len && is_ident_char(chars[pos]) {
            pos += 1;
        }
    } else {
        // Skip non-word, non-whitespace chars
        while pos < len && !is_ident_char(chars[pos]) && !chars[pos].is_whitespace() {
            pos += 1;
        }
    }

    // Skip whitespace between words
    while pos < len && chars[pos].is_whitespace() {
        pos += 1;
    }

    pos.min(len.saturating_sub(1))
}

/// Calculate the previous word boundary position (for `b` key).
///
/// Vim-compatible backward word motion.
pub fn prev_word_boundary(content: &str, column: usize) -> usize {
    let chars: Vec<char> = content.chars().collect();

    if chars.is_empty() || column == 0 {
        return 0;
    }

    let mut pos = column.min(chars.len().saturating_sub(1));

    // Move back one position to look at what's before cursor
    pos = pos.saturating_sub(1);

    // Skip whitespace backwards
    while pos > 0 && chars[pos].is_whitespace() {
        pos -= 1;
    }

    if chars[pos].is_whitespace() {
        return 0;
    }

    // Determine the class of the character we landed on
    let is_word = is_ident_char(chars[pos]);

    if is_word {
        // Skip backwards over word chars
        while pos > 0 && is_ident_char(chars[pos - 1]) {
            pos -= 1;
        }
    } else {
        // Skip backwards over non-word, non-whitespace chars
        while pos > 0 && !is_ident_char(chars[pos - 1]) && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
    }

    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== extract_word_at tests =====

    #[test]
    fn test_extract_word_at_basic() {
        let line = "fn hello_world() {";
        assert_eq!(extract_word_at(line, 3), Some(("hello_world", 3, 14)));
        assert_eq!(extract_word_at(line, 0), Some(("fn", 0, 2)));
    }

    #[test]
    fn test_extract_word_at_beginning() {
        let line = "hello world";
        assert_eq!(extract_word_at(line, 0), Some(("hello", 0, 5)));
    }

    #[test]
    fn test_extract_word_at_end() {
        let line = "hello world";
        assert_eq!(extract_word_at(line, 10), Some(("world", 6, 11)));
    }

    #[test]
    fn test_extract_word_at_on_space() {
        let line = "hello world";
        assert_eq!(extract_word_at(line, 5), None);
    }

    #[test]
    fn test_extract_word_at_on_symbol() {
        let line = "a + b";
        assert_eq!(extract_word_at(line, 2), None);
    }

    #[test]
    fn test_extract_word_at_out_of_bounds() {
        let line = "hello";
        assert_eq!(extract_word_at(line, 100), None);
    }

    #[test]
    fn test_extract_word_at_empty() {
        assert_eq!(extract_word_at("", 0), None);
    }

    #[test]
    fn test_extract_word_at_underscore_prefix() {
        let line = "_private_var";
        assert_eq!(extract_word_at(line, 0), Some(("_private_var", 0, 12)));
    }

    #[test]
    fn test_extract_word_at_number_only() {
        // A number at start doesn't form a valid identifier (no ident_start)
        let line = "123abc";
        assert_eq!(extract_word_at(line, 0), None);
    }

    #[test]
    fn test_extract_word_at_middle_of_word() {
        let line = "some_long_identifier";
        assert_eq!(
            extract_word_at(line, 5),
            Some(("some_long_identifier", 0, 20))
        );
    }

    // ===== is_definition_line tests =====

    #[test]
    fn test_is_definition_line_rust_fn() {
        assert!(is_definition_line("fn main() {", "main"));
        assert!(is_definition_line(
            "pub fn calculate(x: i32) -> i32 {",
            "calculate"
        ));
        assert!(is_definition_line("pub(crate) fn helper() {", "helper"));
    }

    #[test]
    fn test_is_definition_line_rust_struct() {
        assert!(is_definition_line("struct Point {", "Point"));
        assert!(is_definition_line("pub struct Config {", "Config"));
    }

    #[test]
    fn test_is_definition_line_rust_enum() {
        assert!(is_definition_line("enum Color {", "Color"));
        assert!(is_definition_line("pub enum Direction {", "Direction"));
    }

    #[test]
    fn test_is_definition_line_rust_trait() {
        assert!(is_definition_line("trait Display {", "Display"));
        assert!(is_definition_line("pub trait Iterator {", "Iterator"));
    }

    #[test]
    fn test_is_definition_line_rust_impl() {
        assert!(is_definition_line("impl App {", "App"));
        assert!(is_definition_line("impl<T> Vec<T> {", "Vec"));
    }

    #[test]
    fn test_is_definition_line_typescript() {
        assert!(is_definition_line("function render() {", "render"));
        assert!(is_definition_line("export function setup() {", "setup"));
        assert!(is_definition_line("class Component {", "Component"));
        assert!(is_definition_line("interface Props {", "Props"));
        assert!(is_definition_line("export const API_URL =", "API_URL"));
        assert!(is_definition_line("export type Result =", "Result"));
    }

    #[test]
    fn test_is_definition_line_python() {
        assert!(is_definition_line("def process(data):", "process"));
        assert!(is_definition_line("class MyClass:", "MyClass"));
    }

    #[test]
    fn test_is_definition_line_go() {
        assert!(is_definition_line("func main() {", "main"));
        assert!(is_definition_line("type Config struct {", "Config"));
    }

    #[test]
    fn test_is_definition_line_with_indentation() {
        assert!(is_definition_line("    fn nested() {", "nested"));
        assert!(is_definition_line("\t\tpub fn indented() {", "indented"));
    }

    #[test]
    fn test_is_definition_line_false_for_usage() {
        assert!(!is_definition_line("let x = calculate(5);", "calculate"));
        assert!(!is_definition_line("println!(\"{}\", value);", "value"));
    }

    #[test]
    fn test_is_definition_line_word_boundary() {
        // "fn main_loop" should NOT match "main"
        assert!(!is_definition_line("fn main_loop() {", "main"));
        // But "fn main" should match
        assert!(is_definition_line("fn main() {", "main"));
    }

    // ===== extract_all_identifiers tests =====

    #[test]
    fn test_extract_all_identifiers_basic() {
        let ids = extract_all_identifiers("fn hello_world() {");
        let names: Vec<&str> = ids.iter().map(|(w, _, _)| w.as_str()).collect();
        assert_eq!(names, vec!["hello_world"]);
    }

    #[test]
    fn test_extract_all_identifiers_multiple() {
        let ids = extract_all_identifiers("let x = calculate(y, z);");
        let names: Vec<&str> = ids.iter().map(|(w, _, _)| w.as_str()).collect();
        assert_eq!(names, vec!["x", "calculate", "y", "z"]);
    }

    #[test]
    fn test_extract_all_identifiers_dedup() {
        let ids = extract_all_identifiers("foo(foo, bar, foo)");
        let names: Vec<&str> = ids.iter().map(|(w, _, _)| w.as_str()).collect();
        assert_eq!(names, vec!["foo", "bar"]);
    }

    #[test]
    fn test_extract_all_identifiers_empty() {
        let ids = extract_all_identifiers("  + - * / ");
        assert!(ids.is_empty());
    }

    #[test]
    fn test_extract_all_identifiers_skips_keywords() {
        let ids = extract_all_identifiers("pub fn process(data: String) -> Result<()> {");
        let names: Vec<&str> = ids.iter().map(|(w, _, _)| w.as_str()).collect();
        // "pub", "fn" are keywords; "process", "data", "String", "Result" are identifiers
        assert!(names.contains(&"process"));
        assert!(names.contains(&"data"));
        assert!(names.contains(&"String"));
        assert!(names.contains(&"Result"));
        assert!(!names.contains(&"pub"));
        assert!(!names.contains(&"fn"));
    }

    // ===== find_definition_in_patches tests =====

    #[test]
    fn test_find_definition_in_patches_same_file() {
        let files = vec![ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 5,
            deletions: 2,
            patch: Some(
                "@@ -1,3 +1,5 @@\n fn main() {\n+    let x = helper();\n+}\n+fn helper() {\n+    42\n"
                    .to_string(),
            ),
        }];

        let result = find_definition_in_patches("helper", &files, 0);
        assert_eq!(result, Some((0, 4)));
    }

    #[test]
    fn test_find_definition_in_patches_other_file() {
        let files = vec![
            ChangedFile {
                filename: "src/main.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some(
                    "@@ -1,3 +1,4 @@\n fn main() {\n+    let x = helper();\n }\n".to_string(),
                ),
            },
            ChangedFile {
                filename: "src/utils.rs".to_string(),
                status: "added".to_string(),
                additions: 3,
                deletions: 0,
                patch: Some(
                    "@@ -0,0 +1,3 @@\n+pub fn helper() -> i32 {\n+    42\n+}\n".to_string(),
                ),
            },
        ];

        let result = find_definition_in_patches("helper", &files, 0);
        assert_eq!(result, Some((1, 1)));
    }

    #[test]
    fn test_find_definition_in_patches_not_found() {
        let files = vec![ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some(
                "@@ -1,2 +1,3 @@\n fn main() {\n+    println!(\"hello\");\n }\n".to_string(),
            ),
        }];

        let result = find_definition_in_patches("nonexistent", &files, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_definition_in_patches_skips_removed() {
        let files = vec![ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some(
                "@@ -1,3 +1,3 @@\n-fn old_helper() {\n+fn new_helper() {\n     42\n }\n"
                    .to_string(),
            ),
        }];

        // Should NOT find old_helper (removed line)
        let result = find_definition_in_patches("old_helper", &files, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_find_definition_in_patches_skips_none_patch() {
        let files = vec![
            ChangedFile {
                filename: "binary.png".to_string(),
                status: "modified".to_string(),
                additions: 0,
                deletions: 0,
                patch: None,
            },
            ChangedFile {
                filename: "src/lib.rs".to_string(),
                status: "added".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -0,0 +1 @@\n+pub fn target() {}\n".to_string()),
            },
        ];

        let result = find_definition_in_patches("target", &files, 0);
        assert_eq!(result, Some((1, 1)));
    }

    // ===== next_word_boundary tests =====

    #[test]
    fn test_next_word_boundary_basic() {
        let line = "hello world";
        assert_eq!(next_word_boundary(line, 0), 6);
    }

    #[test]
    fn test_next_word_boundary_end_of_line() {
        let line = "hello";
        assert_eq!(next_word_boundary(line, 4), 4);
    }

    #[test]
    fn test_next_word_boundary_from_space() {
        let line = "hello   world";
        assert_eq!(next_word_boundary(line, 5), 8);
    }

    #[test]
    fn test_next_word_boundary_mixed_chars() {
        let line = "fn(hello)";
        // From 'f', skip word "fn", land on non-word "(" (Vim w behavior)
        assert_eq!(next_word_boundary(line, 0), 2);
        // From '(', skip non-word "(", land on word "hello"
        assert_eq!(next_word_boundary(line, 2), 3);
    }

    #[test]
    fn test_next_word_boundary_symbols() {
        let line = "a + b";
        // From 'a', skip word, skip space, land on '+'
        assert_eq!(next_word_boundary(line, 0), 2);
    }

    #[test]
    fn test_next_word_boundary_empty() {
        assert_eq!(next_word_boundary("", 0), 0);
    }

    // ===== prev_word_boundary tests =====

    #[test]
    fn test_prev_word_boundary_basic() {
        let line = "hello world";
        assert_eq!(prev_word_boundary(line, 6), 0);
    }

    #[test]
    fn test_prev_word_boundary_at_start() {
        let line = "hello world";
        assert_eq!(prev_word_boundary(line, 0), 0);
    }

    #[test]
    fn test_prev_word_boundary_from_middle() {
        let line = "hello world test";
        assert_eq!(prev_word_boundary(line, 12), 6);
    }

    #[test]
    fn test_prev_word_boundary_from_word_start() {
        let line = "hello world";
        // From start of "world" (col 6), go back to start of "hello"
        assert_eq!(prev_word_boundary(line, 6), 0);
    }

    #[test]
    fn test_prev_word_boundary_empty() {
        assert_eq!(prev_word_boundary("", 0), 0);
    }

    #[test]
    fn test_prev_word_boundary_symbols() {
        let line = "a + b";
        // From 'b' (col 4), skip space, land on '+'
        assert_eq!(prev_word_boundary(line, 4), 2);
    }

    // ===== is_import_line tests =====

    #[test]
    fn test_is_import_line_rust_simple() {
        assert!(is_import_line("use std::process::Command;", "Command"));
        assert!(is_import_line("use tokio::process::Command;", "Command"));
        assert!(is_import_line("use crate::app::App;", "App"));
    }

    #[test]
    fn test_is_import_line_rust_grouped() {
        assert!(is_import_line(
            "use std::io::{Read, Write, BufRead};",
            "Read"
        ));
        assert!(is_import_line(
            "use std::io::{Read, Write, BufRead};",
            "Write"
        ));
        assert!(is_import_line(
            "use std::io::{Read, Write, BufRead};",
            "BufRead"
        ));
    }

    #[test]
    fn test_is_import_line_rust_indented() {
        assert!(is_import_line("    use std::process::Command;", "Command"));
    }

    #[test]
    fn test_is_import_line_rust_no_match() {
        assert!(!is_import_line("use std::process::Command;", "Process"));
        assert!(!is_import_line("use std::process::Command;", "CommandExt"));
    }

    #[test]
    fn test_is_import_line_js_named() {
        assert!(is_import_line(
            "import { useState, useEffect } from 'react';",
            "useState"
        ));
        assert!(is_import_line(
            "import { useState, useEffect } from 'react';",
            "useEffect"
        ));
    }

    #[test]
    fn test_is_import_line_js_default() {
        assert!(is_import_line("import React from 'react';", "React"));
    }

    #[test]
    fn test_is_import_line_python() {
        assert!(is_import_line("from os.path import join, exists", "join"));
        assert!(is_import_line("from os.path import join, exists", "exists"));
    }

    #[test]
    fn test_is_import_line_not_import() {
        assert!(!is_import_line("fn main() {", "main"));
        assert!(!is_import_line(
            "let cmd = Command::new(\"ls\");",
            "Command"
        ));
    }

    // ===== find_definition_in_patches with imports =====

    #[test]
    fn test_find_definition_in_patches_falls_back_to_import() {
        let files = vec![ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 2,
            deletions: 0,
            patch: Some(
                "@@ -1,3 +1,5 @@\n use std::process::Command;\n \n fn main() {\n+    let cmd = Command::new(\"ls\");\n+    cmd.status().unwrap();\n"
                    .to_string(),
            ),
        }];

        // Command is not defined in the patch, but it is imported
        let result = find_definition_in_patches("Command", &files, 0);
        assert_eq!(result, Some((0, 1))); // line 1 = "use std::process::Command;"
    }

    // ===== find_definition_in_repo is tested with integration tests =====
    // (requires filesystem with grep)

    #[test]
    #[ignore]
    fn test_find_definition_in_repo() {
        // Integration test - requires actual filesystem
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = find_definition_in_repo("main", Path::new(".")).await;
            assert!(result.is_ok());
        });
    }
}
