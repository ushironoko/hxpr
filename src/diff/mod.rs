//! Patch parsing utilities for extracting line information from Git diff patches.
//!
//! This module provides functions to analyze patch content and extract:
//! - Line content without diff prefixes (+/-)
//! - Line type classification (Added, Removed, Context, Header)
//! - New file line numbers for suggestion positioning
//! - Unified diff parsing for splitting multi-file diffs

use std::collections::HashMap;
use tracing::warn;

/// Represents the type of a line in a diff patch
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineType {
    /// Line added in the new version (starts with +)
    Added,
    /// Line removed from the old version (starts with -)
    Removed,
    /// Context line, unchanged (starts with space)
    Context,
    /// Hunk header (@@ ... @@)
    Header,
    /// Metadata lines (diff --, +++, index, etc.)
    Meta,
}

/// Information extracted from a single line in a diff patch
#[derive(Debug, Clone)]
pub struct DiffLineInfo {
    /// The line content without the diff prefix (+/-/space)
    pub line_content: String,
    /// Classification of the line type
    pub line_type: LineType,
    /// Line number in the new file (None for removed lines and headers)
    pub new_line_number: Option<u32>,
}

/// Parse a hunk header to extract the starting line number for new file
/// Format: @@ -old_start,old_count +new_start,new_count @@
fn parse_hunk_header(line: &str) -> Option<u32> {
    // Find the +new_start part
    let plus_pos = line.find('+')?;
    let after_plus = &line[plus_pos + 1..];

    // Extract the number (stop at comma or space)
    let end_pos = after_plus.find([',', ' ']).unwrap_or(after_plus.len());
    let num_str = &after_plus[..end_pos];

    num_str.parse().ok()
}

/// Get information about a specific line in a patch
///
/// # Arguments
/// * `patch` - The full patch content
/// * `line_index` - Zero-based index of the line to analyze
///
/// # Returns
/// * `Some(DiffLineInfo)` - Information about the line if valid
/// * `None` - If the line index is out of bounds
pub fn get_line_info(patch: &str, line_index: usize) -> Option<DiffLineInfo> {
    let lines: Vec<&str> = patch.lines().collect();

    if line_index >= lines.len() {
        return None;
    }

    // Track the current new file line number
    let mut new_line_number: Option<u32> = None;

    for (i, line) in lines.iter().enumerate() {
        let (line_type, content) = classify_line(line);

        // Update line number tracking based on hunk headers
        if line_type == LineType::Header {
            new_line_number = parse_hunk_header(line);
        }

        if i == line_index {
            // For the target line, return the info
            let current_new_line = match line_type {
                LineType::Removed | LineType::Header | LineType::Meta => None,
                _ => new_line_number,
            };

            return Some(DiffLineInfo {
                line_content: content.to_string(),
                line_type,
                new_line_number: current_new_line,
            });
        }

        // Update line numbers for next iteration
        match line_type {
            LineType::Added | LineType::Context => {
                if let Some(n) = new_line_number {
                    new_line_number = Some(n + 1);
                }
            }
            _ => {}
        }
    }

    None
}

/// Classify a line and extract its content without the prefix
pub fn classify_line(line: &str) -> (LineType, &str) {
    if line.starts_with("@@") {
        (LineType::Header, line)
    } else if line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with("diff ")
        || line.starts_with("index ")
    {
        (LineType::Meta, line)
    } else if let Some(content) = line.strip_prefix('+') {
        (LineType::Added, content)
    } else if let Some(content) = line.strip_prefix('-') {
        (LineType::Removed, content)
    } else if let Some(content) = line.strip_prefix(' ') {
        (LineType::Context, content)
    } else {
        // Lines without prefix (shouldn't happen in valid patches, but handle gracefully)
        (LineType::Context, line)
    }
}

/// Check if a line at the given index can have a suggestion
/// Only Added and Context lines can have suggestions
#[allow(dead_code)]
pub fn can_suggest_at_line(patch: &str, line_index: usize) -> bool {
    get_line_info(patch, line_index)
        .map(|info| matches!(info.line_type, LineType::Added | LineType::Context))
        .unwrap_or(false)
}

/// Parse a unified diff output into a map of filename -> patch content
///
/// This function splits the output of `git diff` or `gh pr diff` into individual
/// file patches. The filenames are normalized (without `a/` or `b/` prefixes).
///
/// # Arguments
/// * `unified_diff` - The full unified diff output
///
/// # Returns
/// A HashMap mapping normalized filenames to their patch content
pub fn parse_unified_diff(unified_diff: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let lines: Vec<&str> = unified_diff.lines().collect();

    if lines.is_empty() {
        return result;
    }

    let mut current_filename: Option<String> = None;
    let mut current_patch_start: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("diff --git ") {
            // Save previous file's patch if any
            if let (Some(filename), Some(start)) = (&current_filename, current_patch_start) {
                let patch = lines[start..i].join("\n");
                if !patch.is_empty() {
                    result.insert(filename.clone(), patch);
                }
            }

            // Extract filename for new file
            current_filename = extract_filename(line);
            current_patch_start = Some(i);
        }
    }

    // Save last file's patch
    if let (Some(filename), Some(start)) = (current_filename, current_patch_start) {
        let patch = lines[start..].join("\n");
        if !patch.is_empty() {
            result.insert(filename, patch);
        }
    }

    result
}

/// Extract filename from a "diff --git" line
///
/// Handles various formats:
/// - `diff --git a/src/foo.rs b/src/foo.rs` -> `src/foo.rs`
/// - `diff --git a/file with spaces.rs b/file with spaces.rs` -> `file with spaces.rs`
///
/// For renamed files, returns the new filename (from `b/` path).
fn extract_filename(git_diff_line: &str) -> Option<String> {
    // Format: "diff --git a/{path} b/{path}"
    // We need to find "a/" and "b/" markers and extract the path between them

    let content = git_diff_line.strip_prefix("diff --git ")?;

    // Find "a/" at the start and " b/" separator
    let a_path = content.strip_prefix("a/")?;

    // Find " b/" which separates the two paths
    // Handle case where filename might contain " b/" by finding the last occurrence
    if let Some(b_pos) = a_path.rfind(" b/") {
        let filename = &a_path[..b_pos];
        return Some(filename.to_string());
    }

    warn!("Failed to parse git diff line: {}", git_diff_line);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use std::collections::BTreeMap;

    fn format_parsed_diff(result: &HashMap<String, String>) -> String {
        let sorted: BTreeMap<&str, &str> = result
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut output = String::new();
        for (i, (filename, patch)) in sorted.iter().enumerate() {
            if i > 0 {
                output.push_str("\n---\n");
            }
            output.push_str(&format!("[{}]\n{}", filename, patch));
        }
        output
    }

    const SAMPLE_PATCH: &str = r#"@@ -1,4 +1,5 @@
 line 1
-old line 2
+new line 2
+added line
 line 3"#;

    // Unified diff test data
    const UNIFIED_DIFF_SINGLE: &str = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("Hello");
 }
"#;

    const UNIFIED_DIFF_MULTIPLE: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@
 pub mod app;
+pub mod config;
diff --git a/src/app.rs b/src/app.rs
index 3333333..4444444 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -10,6 +10,7 @@
 struct App {
     name: String,
+    version: String,
 }
"#;

    const UNIFIED_DIFF_NEW_FILE: &str = r#"diff --git a/src/new_file.rs b/src/new_file.rs
new file mode 100644
index 0000000..1234567
--- /dev/null
+++ b/src/new_file.rs
@@ -0,0 +1,3 @@
+fn new_function() {
+    todo!()
+}
"#;

    const UNIFIED_DIFF_DELETED: &str = r#"diff --git a/src/old_file.rs b/src/old_file.rs
deleted file mode 100644
index 1234567..0000000
--- a/src/old_file.rs
+++ /dev/null
@@ -1,3 +0,0 @@
-fn old_function() {
-    todo!()
-}
"#;

    const UNIFIED_DIFF_RENAMED: &str = r#"diff --git a/src/old_name.rs b/src/new_name.rs
similarity index 95%
rename from src/old_name.rs
rename to src/new_name.rs
index 1234567..abcdefg 100644
--- a/src/old_name.rs
+++ b/src/new_name.rs
@@ -1,3 +1,3 @@
-fn old_name() {
+fn new_name() {
 }
"#;

    const UNIFIED_DIFF_BINARY: &str = r#"diff --git a/image.png b/image.png
new file mode 100644
index 0000000..1234567
Binary files /dev/null and b/image.png differ
"#;

    #[test]
    fn test_parse_hunk_header() {
        assert_eq!(parse_hunk_header("@@ -1,4 +1,5 @@"), Some(1));
        assert_eq!(parse_hunk_header("@@ -10,3 +15,7 @@"), Some(15));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some(1));
    }

    #[test]
    fn test_get_line_info_header() {
        let info = get_line_info(SAMPLE_PATCH, 0).unwrap();
        assert_eq!(info.line_type, LineType::Header);
        assert!(info.new_line_number.is_none());
    }

    #[test]
    fn test_get_line_info_context() {
        let info = get_line_info(SAMPLE_PATCH, 1).unwrap();
        assert_eq!(info.line_type, LineType::Context);
        assert_eq!(info.line_content, "line 1");
        assert_eq!(info.new_line_number, Some(1));
    }

    #[test]
    fn test_get_line_info_removed() {
        let info = get_line_info(SAMPLE_PATCH, 2).unwrap();
        assert_eq!(info.line_type, LineType::Removed);
        assert_eq!(info.line_content, "old line 2");
        assert!(info.new_line_number.is_none());
    }

    #[test]
    fn test_get_line_info_added() {
        let info = get_line_info(SAMPLE_PATCH, 3).unwrap();
        assert_eq!(info.line_type, LineType::Added);
        assert_eq!(info.line_content, "new line 2");
        assert_eq!(info.new_line_number, Some(2));
    }

    #[test]
    fn test_can_suggest_at_line() {
        // Header - no
        assert!(!can_suggest_at_line(SAMPLE_PATCH, 0));
        // Context - yes
        assert!(can_suggest_at_line(SAMPLE_PATCH, 1));
        // Removed - no
        assert!(!can_suggest_at_line(SAMPLE_PATCH, 2));
        // Added - yes
        assert!(can_suggest_at_line(SAMPLE_PATCH, 3));
    }

    #[test]
    fn test_classify_line_no_prefix() {
        // diff プレフィックスなし → Context にフォールバック (L123-125)
        let (line_type, content) = classify_line("no prefix");
        assert_eq!(line_type, LineType::Context);
        assert_eq!(content, "no prefix");
    }

    #[test]
    fn test_classify_line_empty() {
        // 空文字列 → Context にフォールバック (L123-125)
        let (line_type, content) = classify_line("");
        assert_eq!(line_type, LineType::Context);
        assert_eq!(content, "");
    }

    #[test]
    fn test_parse_hunk_header_no_comma_no_space() {
        // "@@ -1 +42\ntest" → after_plus = "42" で find([',', ' ']) が None
        // → unwrap_or(after_plus.len()) に到達 (L46)
        let patch = "@@ -1 +42\ntest";
        let info = get_line_info(patch, 1).unwrap();
        assert_eq!(info.line_type, LineType::Context);
        assert_eq!(info.new_line_number, Some(42));
    }

    #[test]
    fn test_out_of_bounds() {
        assert!(get_line_info(SAMPLE_PATCH, 100).is_none());
    }

    // ============================================
    // Unified diff parser tests
    // ============================================

    #[test]
    fn test_extract_filename() {
        assert_eq!(
            extract_filename("diff --git a/src/foo.rs b/src/foo.rs"),
            Some("src/foo.rs".to_string())
        );
        assert_eq!(
            extract_filename("diff --git a/main.rs b/main.rs"),
            Some("main.rs".to_string())
        );
        assert_eq!(
            extract_filename("diff --git a/deep/nested/path/file.rs b/deep/nested/path/file.rs"),
            Some("deep/nested/path/file.rs".to_string())
        );
    }

    #[test]
    fn test_extract_filename_renamed() {
        // For renamed files, we use the "a/" path (old name) because
        // GitHub API returns the old filename in its response
        assert_eq!(
            extract_filename("diff --git a/src/old_name.rs b/src/new_name.rs"),
            Some("src/old_name.rs".to_string())
        );
    }

    #[test]
    fn test_extract_filename_invalid() {
        assert_eq!(extract_filename("not a diff line"), None);
        assert_eq!(extract_filename("diff something else"), None);
    }

    #[test]
    fn test_extract_filename_no_b_separator() {
        // " b/" が存在しない場合 → warn! パスを通って None
        assert_eq!(extract_filename("diff --git a/file nob"), None);
    }

    #[test]
    fn test_parse_single_file() {
        let result = parse_unified_diff(UNIFIED_DIFF_SINGLE);
        assert_snapshot!(format_parsed_diff(&result), @r#"
        [src/main.rs]
        diff --git a/src/main.rs b/src/main.rs
        index 1234567..abcdefg 100644
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1,3 +1,4 @@
         fn main() {
        +    println!("Hello");
         }
        "#);
    }

    #[test]
    fn test_parse_multiple_files() {
        let result = parse_unified_diff(UNIFIED_DIFF_MULTIPLE);
        assert_snapshot!(format_parsed_diff(&result), @r#"
        [src/app.rs]
        diff --git a/src/app.rs b/src/app.rs
        index 3333333..4444444 100644
        --- a/src/app.rs
        +++ b/src/app.rs
        @@ -10,6 +10,7 @@
         struct App {
             name: String,
        +    version: String,
         }
        ---
        [src/lib.rs]
        diff --git a/src/lib.rs b/src/lib.rs
        index 1111111..2222222 100644
        --- a/src/lib.rs
        +++ b/src/lib.rs
        @@ -1,2 +1,3 @@
         pub mod app;
        +pub mod config;
        "#);
    }

    #[test]
    fn test_parse_new_file() {
        let result = parse_unified_diff(UNIFIED_DIFF_NEW_FILE);
        assert_snapshot!(format_parsed_diff(&result), @r#"
        [src/new_file.rs]
        diff --git a/src/new_file.rs b/src/new_file.rs
        new file mode 100644
        index 0000000..1234567
        --- /dev/null
        +++ b/src/new_file.rs
        @@ -0,0 +1,3 @@
        +fn new_function() {
        +    todo!()
        +}
        "#);
    }

    #[test]
    fn test_parse_deleted_file() {
        let result = parse_unified_diff(UNIFIED_DIFF_DELETED);
        assert_snapshot!(format_parsed_diff(&result), @r#"
        [src/old_file.rs]
        diff --git a/src/old_file.rs b/src/old_file.rs
        deleted file mode 100644
        index 1234567..0000000
        --- a/src/old_file.rs
        +++ /dev/null
        @@ -1,3 +0,0 @@
        -fn old_function() {
        -    todo!()
        -}
        "#);
    }

    #[test]
    fn test_parse_renamed_file() {
        let result = parse_unified_diff(UNIFIED_DIFF_RENAMED);
        // Uses old filename (from a/ path) for matching with GitHub API
        assert_snapshot!(format_parsed_diff(&result), @r#"
        [src/old_name.rs]
        diff --git a/src/old_name.rs b/src/new_name.rs
        similarity index 95%
        rename from src/old_name.rs
        rename to src/new_name.rs
        index 1234567..abcdefg 100644
        --- a/src/old_name.rs
        +++ b/src/new_name.rs
        @@ -1,3 +1,3 @@
        -fn old_name() {
        +fn new_name() {
         }
        "#);
    }

    #[test]
    fn test_parse_binary_file() {
        let result = parse_unified_diff(UNIFIED_DIFF_BINARY);
        assert_snapshot!(format_parsed_diff(&result), @r#"
        [image.png]
        diff --git a/image.png b/image.png
        new file mode 100644
        index 0000000..1234567
        Binary files /dev/null and b/image.png differ
        "#);
    }

    #[test]
    fn test_parse_empty_diff() {
        let result = parse_unified_diff("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_filename_matches_github_api_format() {
        // GitHub API returns filenames without "a/" or "b/" prefix
        // Our parser should return filenames in the same format
        let result = parse_unified_diff(UNIFIED_DIFF_SINGLE);
        let filename = result.keys().next().unwrap();

        // Should not have "a/" or "b/" prefix
        assert!(!filename.starts_with("a/"));
        assert!(!filename.starts_with("b/"));

        // Should match the format GitHub API returns
        assert_eq!(filename, "src/main.rs");
    }
}
