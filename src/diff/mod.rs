//! Patch parsing utilities for extracting line information from Git diff patches.
//!
//! This module provides functions to analyze patch content and extract:
//! - Line content without diff prefixes (+/-)
//! - Line type classification (Added, Removed, Context, Header)
//! - New file line numbers for suggestion positioning

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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PATCH: &str = r#"@@ -1,4 +1,5 @@
 line 1
-old line 2
+new line 2
+added line
 line 3"#;

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
    fn test_out_of_bounds() {
        assert!(get_line_info(SAMPLE_PATCH, 100).is_none());
    }
}
