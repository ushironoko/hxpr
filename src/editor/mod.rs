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

/// Resolve editor command and split into program + arguments.
///
/// Resolution order (same as git):
///   1. Explicit config value (`configured`)
///   2. `$VISUAL`
///   3. `$EDITOR`
///   4. `"vi"` (fallback)
///
/// Supports quoted arguments (e.g. `emacsclient -c -a ""`) via `shell_words::split`.
fn resolve_and_split_editor(configured: Option<&str>) -> Result<(String, Vec<String>)> {
    let raw = configured
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| env::var("VISUAL").ok().filter(|s| !s.is_empty()))
        .or_else(|| env::var("EDITOR").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "vi".to_string());

    let parts = shell_words::split(&raw)?;
    let cmd = parts
        .first()
        .ok_or_else(|| anyhow::anyhow!("empty editor command"))?
        .clone();
    let args = parts[1..].to_vec();
    Ok((cmd, args))
}

/// Run a `Command`, converting `NotFound` into a user-friendly error message.
fn run_editor_command(cmd: &str, mut command: Command) -> Result<std::process::ExitStatus> {
    command.status().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!(
                "Editor '{}' not found. Set $VISUAL or $EDITOR environment variable, \
                 or set 'editor' in ~/.config/octorus/config.toml",
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

    #[test]
    fn test_resolve_explicit_config() {
        let (cmd, args) = resolve_and_split_editor(Some("vim")).unwrap();
        assert_eq!(cmd, "vim");
        assert!(args.is_empty());
    }

    #[test]
    fn test_resolve_with_args() {
        let (cmd, args) = resolve_and_split_editor(Some("code --wait")).unwrap();
        assert_eq!(cmd, "code");
        assert_eq!(args, vec!["--wait"]);
    }

    #[test]
    fn test_resolve_with_quoted_args() {
        let (cmd, args) =
            resolve_and_split_editor(Some(r#"emacsclient -c -a """#)).unwrap();
        assert_eq!(cmd, "emacsclient");
        assert_eq!(args, vec!["-c", "-a", ""]);
    }

    #[test]
    fn test_resolve_extra_whitespace() {
        let (cmd, args) = resolve_and_split_editor(Some("  vim   --noplugin  ")).unwrap();
        assert_eq!(cmd, "vim");
        assert_eq!(args, vec!["--noplugin"]);
    }

    #[test]
    fn test_resolve_empty_string_falls_through() {
        // Empty string should be treated as "not configured" and fall through
        // to env vars. We can't easily test env var fallback without serial,
        // so just ensure it doesn't panic.
        let result = resolve_and_split_editor(Some(""));
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_none_falls_through() {
        // None should fall through to env vars / "vi" default
        let result = resolve_and_split_editor(None);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_resolve_visual_env_var() {
        // Save and clear
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::set_var("VISUAL", "nano");
        env::remove_var("EDITOR");

        let (cmd, args) = resolve_and_split_editor(None).unwrap();
        assert_eq!(cmd, "nano");
        assert!(args.is_empty());

        // Restore
        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    #[serial]
    fn test_resolve_editor_env_var() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::set_var("EDITOR", "emacs");

        let (cmd, args) = resolve_and_split_editor(None).unwrap();
        assert_eq!(cmd, "emacs");
        assert!(args.is_empty());

        // Restore
        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    #[serial]
    fn test_resolve_visual_takes_priority_over_editor() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::set_var("VISUAL", "code --wait");
        env::set_var("EDITOR", "vim");

        let (cmd, args) = resolve_and_split_editor(None).unwrap();
        assert_eq!(cmd, "code");
        assert_eq!(args, vec!["--wait"]);

        // Restore
        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    #[serial]
    fn test_resolve_config_takes_priority_over_env() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::set_var("VISUAL", "nano");
        env::set_var("EDITOR", "emacs");

        let (cmd, args) = resolve_and_split_editor(Some("hx")).unwrap();
        assert_eq!(cmd, "hx");
        assert!(args.is_empty());

        // Restore
        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    #[serial]
    fn test_resolve_fallback_to_vi() {
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();
        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let (cmd, args) = resolve_and_split_editor(None).unwrap();
        assert_eq!(cmd, "vi");
        assert!(args.is_empty());

        // Restore
        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

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
}
