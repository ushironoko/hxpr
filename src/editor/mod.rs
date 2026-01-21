use anyhow::Result;
use std::env;
use std::fs;
use std::process::Command;
use tempfile::NamedTempFile;

/// Open external editor for comment input
pub fn open_comment_editor(editor: &str, filename: &str, line: usize) -> Result<Option<String>> {
    let temp_file = NamedTempFile::new()?;
    let template = format!(
        "<!-- octorus: Enter your comment below -->\n\
         <!-- File: {} Line: {} -->\n\
         <!-- Save and close to submit, delete all content to cancel -->\n\n",
        filename, line
    );
    fs::write(temp_file.path(), &template)?;

    let editor_cmd = resolve_editor(editor);
    let status = Command::new(&editor_cmd).arg(temp_file.path()).status()?;

    if !status.success() {
        return Ok(None);
    }

    let content = fs::read_to_string(temp_file.path())?;
    let body = extract_comment_body(&content);

    if body.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(body))
    }
}

/// Open external editor for review submission
pub fn open_review_editor(editor: &str) -> Result<Option<String>> {
    let temp_file = NamedTempFile::new()?;
    let template = "<!-- Enter your review comment -->\n\
                    <!-- Save and close to submit -->\n\n";
    fs::write(temp_file.path(), template)?;

    let editor_cmd = resolve_editor(editor);
    let status = Command::new(&editor_cmd).arg(temp_file.path()).status()?;

    if !status.success() {
        return Ok(None);
    }

    let content = fs::read_to_string(temp_file.path())?;
    let body = extract_comment_body(&content);

    if body.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(body))
    }
}

fn resolve_editor(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    env::var("EDITOR").unwrap_or_else(|_| "hx".to_string())
}

fn extract_comment_body(content: &str) -> String {
    content
        .lines()
        .filter(|line| !line.trim().starts_with("<!--"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Open external editor for suggestion input
/// Returns the suggested code (without the original template comments)
pub fn open_suggestion_editor(
    editor: &str,
    filename: &str,
    line: usize,
    original_code: &str,
) -> Result<Option<String>> {
    let temp_file = NamedTempFile::new()?;
    let template = format!(
        "<!-- octorus: Edit the code below to create a suggestion -->\n\
         <!-- File: {} Line: {} -->\n\
         <!-- Save and close to submit, delete all content to cancel -->\n\n\
         {}",
        filename, line, original_code
    );
    fs::write(temp_file.path(), &template)?;

    let editor_cmd = resolve_editor(editor);
    let status = Command::new(&editor_cmd).arg(temp_file.path()).status()?;

    if !status.success() {
        return Ok(None);
    }

    let content = fs::read_to_string(temp_file.path())?;
    let suggested = extract_comment_body(&content);

    if suggested.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(suggested))
    }
}
