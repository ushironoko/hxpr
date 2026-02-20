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

/// Check whether a command can be found in PATH and is executable.
fn command_found_in_path(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Generate editor candidates in priority order.
/// Resolution order (same as git): config → $VISUAL → $EDITOR → vi
fn editor_candidates(configured: Option<&str>) -> Vec<String> {
    [
        configured
            .filter(|s| !s.trim().is_empty())
            .map(String::from),
        env::var("VISUAL").ok().filter(|s| !s.trim().is_empty()),
        env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()),
        Some("vi".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Resolve editor command and split into program + arguments.
///
/// Resolution order (same as git):
///   1. Explicit config value (`configured`)
///   2. `$VISUAL`
///   3. `$EDITOR`
///   4. `"vi"` (fallback)
///
/// Each candidate is checked for PATH availability. If not found, the next
/// candidate is tried. If no candidate is found in PATH, the first candidate
/// is returned (will produce a user-friendly error at execution time).
///
/// Supports quoted arguments (e.g. `emacsclient -c -a ""`) via `shell_words::split`.
fn resolve_and_split_editor(configured: Option<&str>) -> Result<(String, Vec<String>)> {
    let candidates = editor_candidates(configured);
    let mut first_parsed: Option<(String, Vec<String>)> = None;
    let mut skipped: Vec<String> = Vec::new();

    for raw in &candidates {
        let parts = shell_words::split(raw)?;
        let Some(cmd) = parts.first() else { continue };
        let parsed = (cmd.clone(), parts[1..].to_vec());

        if first_parsed.is_none() {
            first_parsed = Some(parsed.clone());
        }

        if command_found_in_path(cmd) {
            if !skipped.is_empty() {
                tracing::warn!(
                    skipped_editors = ?skipped,
                    resolved_editor = %cmd,
                    "editor candidate not found in PATH, falling back"
                );
            }
            return Ok(parsed);
        }

        skipped.push(cmd.clone());
    }

    // All candidates missing from PATH – return the first one
    // (will produce a NotFound error at execution time)
    Ok(first_parsed.unwrap_or_else(|| ("vi".to_string(), vec![])))
}

/// Run a `Command`, converting `NotFound` into a user-friendly error message.
fn run_editor_command(cmd: &str, mut command: Command) -> Result<std::process::ExitStatus> {
    command.status().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(
                "Editor '{}' not found (also checked $VISUAL and $EDITOR). \
                 Set 'editor' in ~/.config/octorus/config.toml to an installed editor.",
                cmd
            )
        } else {
            anyhow::anyhow!("Failed to launch editor '{}': {}", cmd, e)
        }
    })
}

/// ジェネリックエディタ関数（内部用）
fn open_editor_internal(
    editor: Option<&str>,
    template: EditorTemplate<'_>,
) -> Result<Option<String>> {
    let temp_file = NamedTempFile::new()?;

    let content = if let Some(initial) = template.initial_content {
        format!("{}\n\n{}", template.header, initial)
    } else {
        format!("{}\n\n", template.header)
    };

    fs::write(temp_file.path(), &content)?;

    let (cmd, args) = resolve_and_split_editor(editor)?;
    let mut command = Command::new(&cmd);
    command.args(&args).arg(temp_file.path());
    let status = run_editor_command(&cmd, command)?;

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
pub fn open_comment_editor(
    editor: Option<&str>,
    filename: &str,
    line: usize,
) -> Result<Option<String>> {
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
pub fn open_review_editor(editor: Option<&str>) -> Result<Option<String>> {
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
    editor: Option<&str>,
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

/// Open external editor at a specific file and line number.
///
/// Uses the format `$EDITOR +{line} {file_path}` to open the file.
/// The caller is responsible for suspending/restoring the TUI terminal.
pub fn open_file_at_line(editor: Option<&str>, file_path: &str, line: usize) -> Result<()> {
    let (cmd, args) = resolve_and_split_editor(editor)?;
    let mut command = Command::new(&cmd);
    command.args(&args).arg(format!("+{}", line)).arg(file_path);
    let status = run_editor_command(&cmd, command)?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    Ok(())
}

/// Open external editor for AI Rally clarification response
/// Returns the user's answer to the clarification question
pub fn open_clarification_editor(editor: Option<&str>, question: &str) -> Result<Option<String>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // ── command_found_in_path tests ──

    #[test]
    fn test_command_found_in_path_basic() {
        // `sh` exists on every Unix system
        assert!(command_found_in_path("sh"));
        assert!(!command_found_in_path("__nonexistent__"));
    }

    // ── editor_candidates tests (PATH-independent, pure logic) ──

    #[test]
    #[serial]
    fn test_candidates_explicit_config() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let candidates = editor_candidates(Some("vim"));
        assert_eq!(candidates[0], "vim");

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_empty_string_falls_through() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let candidates = editor_candidates(Some(""));
        // Empty config is skipped, only "vi" remains
        assert_eq!(candidates, vec!["vi"]);

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_whitespace_only_falls_through() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let candidates = editor_candidates(Some("   "));
        assert_eq!(candidates, vec!["vi"]);

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_none_falls_through() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let candidates = editor_candidates(None);
        assert_eq!(candidates, vec!["vi"]);

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_visual_env_var() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::set_var("VISUAL", "nano");
        env::remove_var("EDITOR");

        let candidates = editor_candidates(None);
        assert_eq!(candidates, vec!["nano", "vi"]);

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_editor_env_var() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::set_var("EDITOR", "emacs");

        let candidates = editor_candidates(None);
        assert_eq!(candidates, vec!["emacs", "vi"]);

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_visual_takes_priority_over_editor() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::set_var("VISUAL", "code --wait");
        env::set_var("EDITOR", "vim");

        let candidates = editor_candidates(None);
        assert_eq!(candidates, vec!["code --wait", "vim", "vi"]);

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_config_takes_priority_over_env() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::set_var("VISUAL", "nano");
        env::set_var("EDITOR", "emacs");

        let candidates = editor_candidates(Some("hx"));
        assert_eq!(candidates[0], "hx");
        assert_eq!(candidates[1], "nano");
        assert_eq!(candidates[2], "emacs");

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_candidates_fallback_to_vi() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let candidates = editor_candidates(None);
        assert_eq!(candidates, vec!["vi"]);

        restore_env(orig_visual, orig_editor);
    }

    // ── shell_words parsing tests (PATH-independent) ──

    #[test]
    fn test_parse_with_args() {
        let parts = shell_words::split("code --wait").unwrap();
        assert_eq!(parts, vec!["code", "--wait"]);
    }

    #[test]
    fn test_parse_with_quoted_args() {
        let parts = shell_words::split(r#"emacsclient -c -a """#).unwrap();
        assert_eq!(parts, vec!["emacsclient", "-c", "-a", ""]);
    }

    #[test]
    fn test_parse_extra_whitespace() {
        let parts = shell_words::split("  vim   --noplugin  ").unwrap();
        assert_eq!(parts, vec!["vim", "--noplugin"]);
    }

    // ── resolve_and_split_editor tests (uses `sh` which exists on all Unix) ──

    #[test]
    #[serial]
    fn test_resolve_finds_sh() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let (cmd, args) = resolve_and_split_editor(Some("sh")).unwrap();
        assert_eq!(cmd, "sh");
        assert!(args.is_empty());

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_fallback_when_configured_not_in_path() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::set_var("EDITOR", "sh");

        // Configured editor doesn't exist, should fall back to $EDITOR=sh
        let (cmd, args) = resolve_and_split_editor(Some("__nonexistent__")).unwrap();
        assert_eq!(cmd, "sh");
        assert!(args.is_empty());

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_fallback_with_args_not_inherited() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::set_var("EDITOR", "sh");

        // Args from the non-existent editor should NOT carry over
        let (cmd, args) =
            resolve_and_split_editor(Some("__nonexistent__ --flag")).unwrap();
        assert_eq!(cmd, "sh");
        assert!(args.is_empty());

        restore_env(orig_visual, orig_editor);
    }

    #[test]
    #[serial]
    fn test_all_candidates_not_in_path() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        let orig_path = env::var("PATH").ok();
        env::set_var("VISUAL", "__nonexistent_visual__");
        env::set_var("EDITOR", "__nonexistent_editor__");
        // Point PATH to an empty dir so even `vi` is not found
        let empty_dir = tempfile::tempdir().unwrap();
        env::set_var("PATH", empty_dir.path());

        // All candidates missing from PATH → returns the first parsed one
        let (cmd, _) =
            resolve_and_split_editor(Some("__nonexistent_config__")).unwrap();
        assert_eq!(cmd, "__nonexistent_config__");

        // Restore
        match orig_path {
            Some(v) => env::set_var("PATH", v),
            None => env::remove_var("PATH"),
        }
        restore_env(orig_visual, orig_editor);
    }

    // ── run_editor_command error message test ──

    #[test]
    fn test_run_editor_command_not_found() {
        let command = Command::new("__octorus_nonexistent_editor__");
        let err = run_editor_command("__octorus_nonexistent_editor__", command)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not found"),
            "expected 'not found' in error message, got: {}",
            msg
        );
        assert!(msg.contains("$VISUAL"));
        assert!(msg.contains("$EDITOR"));
        assert!(msg.contains("config.toml"));
    }

    // ── helpers ──

    fn restore_env(orig_visual: Option<String>, orig_editor: Option<String>) {
        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }
}
