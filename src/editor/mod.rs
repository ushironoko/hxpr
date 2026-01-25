use anyhow::Result;
use std::borrow::Cow;
use std::env;
use std::fs;
use std::process::Command;
use tempfile::NamedTempFile;

/// エディタのテンプレート設定
struct EditorTemplate<'a> {
    header: Cow<'a, str>,
    initial_content: Option<Cow<'a, str>>,
}

/// ジェネリックエディタ関数（内部用）
fn open_editor_internal(editor: &str, template: EditorTemplate<'_>) -> Result<Option<String>> {
    let temp_file = NamedTempFile::new()?;

    let content = if let Some(initial) = template.initial_content {
        format!("{}\n\n{}", template.header, initial)
    } else {
        format!("{}\n\n", template.header)
    };

    fs::write(temp_file.path(), &content)?;

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

/// Open external editor for comment input
pub fn open_comment_editor(editor: &str, filename: &str, line: usize) -> Result<Option<String>> {
    open_editor_internal(
        editor,
        EditorTemplate {
            header: Cow::Owned(format!(
                "<!-- octorus: Enter your comment below -->\n\
                 <!-- File: {} Line: {} -->\n\
                 <!-- Save and close to submit, delete all content to cancel -->",
                filename, line
            )),
            initial_content: None,
        },
    )
}

/// Open external editor for review submission
pub fn open_review_editor(editor: &str) -> Result<Option<String>> {
    open_editor_internal(
        editor,
        EditorTemplate {
            header: Cow::Borrowed(
                "<!-- Enter your review comment -->\n\
                 <!-- Save and close to submit -->",
            ),
            initial_content: None,
        },
    )
}

fn resolve_editor(configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }
    env::var("EDITOR").unwrap_or_else(|_| "vi".to_string())
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
    open_editor_internal(
        editor,
        EditorTemplate {
            header: Cow::Owned(format!(
                "<!-- octorus: Edit the code below to create a suggestion -->\n\
                 <!-- File: {} Line: {} -->\n\
                 <!-- Save and close to submit, delete all content to cancel -->",
                filename, line
            )),
            initial_content: Some(Cow::Borrowed(original_code)),
        },
    )
}

/// Open external editor for AI Rally clarification response
/// Returns the user's answer to the clarification question
pub fn open_clarification_editor(editor: &str, question: &str) -> Result<Option<String>> {
    open_editor_internal(
        editor,
        EditorTemplate {
            header: Cow::Owned(format!(
                "<!-- octorus: AI Rally Clarification -->\n\
                 <!-- Question: {} -->\n\
                 <!-- Enter your answer below. Save and close to submit. -->\n\
                 <!-- Delete all content to cancel. -->",
                question
            )),
            initial_content: None,
        },
    )
}
