use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::AiConfig;

use super::adapter::{Context, ReviewAction, ReviewerOutput};

/// Source of a resolved prompt template
#[derive(Debug, Clone, PartialEq)]
pub enum PromptSource {
    /// .octorus/prompts/ (project-local)
    Local(PathBuf),
    /// config.prompt_dir (explicit path)
    PromptDir(PathBuf),
    /// ~/.config/octorus/prompts/ (global)
    Global(PathBuf),
    /// Binary-embedded default
    Embedded,
}

/// Default prompt templates embedded in the binary
mod defaults {
    pub const REVIEWER: &str = include_str!("defaults/reviewer.md");
    pub const REVIEWEE: &str = include_str!("defaults/reviewee.md");
    pub const REREVIEW: &str = include_str!("defaults/rereview.md");
}

/// Prompt loader that reads templates from files or uses defaults.
///
/// Resolution order (highest priority first):
/// 1. `.octorus/prompts/{file}` — project-local
/// 2. `config.prompt_dir` — explicit path from merged config
/// 3. `~/.config/octorus/prompts/{file}` — global default
/// 4. Binary-embedded default — fallback
pub struct PromptLoader {
    prompt_dir: Option<PathBuf>,
    local_prompts_dir: Option<PathBuf>,
    global_prompts_dir: Option<PathBuf>,
}

impl PromptLoader {
    /// Create a new PromptLoader with the given config and project root
    pub fn new(config: &AiConfig, project_root: &Path) -> Self {
        // Resolve prompt_dir: make relative paths absolute against project_root
        let prompt_dir = config.prompt_dir.as_ref().map(|p| {
            let path = PathBuf::from(p);
            if path.is_absolute() {
                path
            } else {
                project_root.join(path)
            }
        });

        let local_prompts_dir = {
            let path = project_root.join(".octorus/prompts");
            if path.is_dir() {
                Some(path)
            } else {
                None
            }
        };

        let global_prompts_dir = xdg::BaseDirectories::with_prefix("octorus")
            .ok()
            .map(|dirs| dirs.get_config_home().join("prompts"));

        Self {
            prompt_dir,
            local_prompts_dir,
            global_prompts_dir,
        }
    }

    /// Resolve which source would be used for a given prompt filename.
    pub fn resolve_source(&self, filename: &str) -> PromptSource {
        if let Some(ref dir) = self.local_prompts_dir {
            let path = dir.join(filename);
            if path.exists() {
                return PromptSource::Local(path);
            }
        }
        if let Some(ref dir) = self.prompt_dir {
            let path = dir.join(filename);
            if path.exists() {
                return PromptSource::PromptDir(path);
            }
        }
        if let Some(ref dir) = self.global_prompts_dir {
            let path = dir.join(filename);
            if path.exists() {
                return PromptSource::Global(path);
            }
        }
        PromptSource::Embedded
    }

    /// Resolve sources for all standard prompt files.
    pub fn resolve_all_sources(&self) -> Vec<(String, PromptSource)> {
        ["reviewer.md", "reviewee.md", "rereview.md"]
            .iter()
            .map(|f| (f.to_string(), self.resolve_source(f)))
            .collect()
    }

    /// Load the reviewer prompt with variable substitution
    pub fn load_reviewer_prompt(&self, context: &Context, iteration: u32) -> String {
        let template = self.load_template("reviewer.md", defaults::REVIEWER);

        let pr_body = context
            .pr_body
            .as_deref()
            .unwrap_or("(No description provided)");

        let mut vars = HashMap::new();
        vars.insert("repo", context.repo.clone());
        vars.insert("pr_number", context.pr_number.to_string());
        vars.insert("pr_title", context.pr_title.clone());
        vars.insert("pr_body", pr_body.to_string());
        vars.insert("diff", context.diff.clone());
        vars.insert("iteration", iteration.to_string());

        render_template(&template, &vars)
    }

    /// Load the reviewee prompt with variable substitution
    pub fn load_reviewee_prompt(
        &self,
        context: &Context,
        review: &ReviewerOutput,
        iteration: u32,
    ) -> String {
        let template = self.load_template("reviewee.md", defaults::REVIEWEE);

        let comments_text = review
            .comments
            .iter()
            .map(|c| {
                format!(
                    "- [{severity:?}] {path}:{line}: {body}",
                    severity = c.severity,
                    path = c.path,
                    line = c.line,
                    body = c.body
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let blocking_text = if review.blocking_issues.is_empty() {
            "None".to_string()
        } else {
            review
                .blocking_issues
                .iter()
                .map(|i| format!("- {}", i))
                .collect::<Vec<_>>()
                .join("\n")
        };

        // External comments section (from Copilot, CodeRabbit, etc.)
        let external_section = if context.external_comments.is_empty() {
            String::new()
        } else {
            let text = context
                .external_comments
                .iter()
                .map(|c| {
                    let location = c
                        .path
                        .as_ref()
                        .map(|p| {
                            c.line
                                .map(|l| format!("{}:{}", p, l))
                                .unwrap_or_else(|| p.clone())
                        })
                        .unwrap_or_else(|| "general".to_string());
                    format!("- [{}] {}: {}", c.source, location, truncate(&c.body, 200))
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                r#"

## External Tool Feedback

The following comments are from external code review tools (Copilot, CodeRabbit, etc.):

{text}

Note: Address these comments if they are relevant and valid. Don't wait for more feedback from these tools.
"#,
                text = text
            )
        };

        let review_action = match review.action {
            ReviewAction::Approve => "Approve",
            ReviewAction::RequestChanges => "RequestChanges",
            ReviewAction::Comment => "Comment",
        };

        let git_operations = if context.local_mode {
            "## Git Operations\n\n\
             This is a LOCAL-ONLY session. Do NOT run any git write commands \
             (add, commit, push, stash, switch, branch, merge, rebase, reset, etc.).\n\
             Only read-only git commands (status, diff, log, show) are allowed.\n\
             Edit files directly — the user will handle staging and committing."
        } else {
            "## Git Operations\n\n\
             After making changes, you MUST commit your changes locally:\n\n\
             1. Check status: `git status`\n\
             2. Stage files: `git add <files>`\n\
             3. Commit: `git commit -m \"fix: <description>\"`\n\n\
             NOTE: Do NOT push changes. The user will review and push manually.\n\
             If git push is needed and allowed, it will be explicitly permitted via config."
        };

        let mut vars = HashMap::new();
        vars.insert("repo", context.repo.clone());
        vars.insert("pr_number", context.pr_number.to_string());
        vars.insert("pr_title", context.pr_title.clone());
        vars.insert("iteration", iteration.to_string());
        vars.insert("review_summary", review.summary.clone());
        vars.insert("review_action", review_action.to_string());
        vars.insert("review_comments", comments_text);
        vars.insert("blocking_issues", blocking_text);
        vars.insert("external_comments", external_section);
        vars.insert("git_operations", git_operations.to_string());

        render_template(&template, &vars)
    }

    /// Load the re-review prompt with variable substitution
    pub fn load_rereview_prompt(
        &self,
        context: &Context,
        iteration: u32,
        changes_summary: &str,
        updated_diff: &str,
    ) -> String {
        let template = self.load_template("rereview.md", defaults::REREVIEW);

        let mut vars = HashMap::new();
        vars.insert("repo", context.repo.clone());
        vars.insert("pr_number", context.pr_number.to_string());
        vars.insert("pr_title", context.pr_title.clone());
        vars.insert("iteration", iteration.to_string());
        vars.insert("changes_summary", changes_summary.to_string());
        vars.insert("updated_diff", updated_diff.to_string());

        render_template(&template, &vars)
    }

    /// Load a template with multi-level resolution.
    ///
    /// Order: local .octorus/prompts/ → config.prompt_dir → global prompts → embedded default
    fn load_template(&self, filename: &str, default: &str) -> String {
        // 1. Project-local .octorus/prompts/ (highest priority)
        if let Some(content) = Self::try_load_from(&self.local_prompts_dir, filename) {
            return content;
        }
        // 2. config.prompt_dir (explicit path)
        if let Some(content) = Self::try_load_from(&self.prompt_dir, filename) {
            return content;
        }
        // 3. Global ~/.config/octorus/prompts/
        if let Some(content) = Self::try_load_from(&self.global_prompts_dir, filename) {
            return content;
        }
        // 4. Binary-embedded default
        default.to_string()
    }

    /// Try to load a file from an optional directory.
    /// Returns None for NotFound; logs a warning and returns None for other errors.
    fn try_load_from(dir: &Option<PathBuf>, filename: &str) -> Option<String> {
        dir.as_ref().and_then(|d| {
            let path = d.join(filename);
            match fs::read_to_string(&path) {
                Ok(content) => Some(content),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to read prompt '{}' from {}: {}",
                        filename,
                        path.display(),
                        e
                    );
                    None
                }
            }
        })
    }
}

/// Render a template by replacing {{key}} with values from vars
fn render_template(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Truncate a string to a maximum length (UTF-8 safe)
fn truncate(s: &str, max_len: usize) -> String {
    let s = s.trim();
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s.chars().take(max_len).collect()
    } else {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::{CommentSeverity, ExternalComment, ReviewComment};

    fn create_test_context() -> Context {
        Context {
            repo: "owner/repo".to_string(),
            pr_number: 123,
            pr_title: "Add feature".to_string(),
            pr_body: Some("This adds a new feature".to_string()),
            diff: "+added line\n-removed line".to_string(),
            working_dir: None,
            head_sha: "abc123".to_string(),
            base_branch: "main".to_string(),
            external_comments: Vec::new(),
            local_mode: false,
            file_patches: Vec::new(),
        }
    }

    #[test]
    fn test_render_template() {
        let template = "Hello {{name}}, you have {{count}} messages.";
        let mut vars = HashMap::new();
        vars.insert("name", "Alice".to_string());
        vars.insert("count", "5".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "Hello Alice, you have 5 messages.");
    }

    #[test]
    fn test_render_template_missing_var() {
        let template = "Hello {{name}}, {{unknown}} variable.";
        let mut vars = HashMap::new();
        vars.insert("name", "Bob".to_string());

        let result = render_template(template, &vars);
        assert_eq!(result, "Hello Bob, {{unknown}} variable.");
    }

    #[test]
    fn test_load_reviewer_prompt() {
        let config = AiConfig::default();
        let loader = PromptLoader::new(&config, Path::new("/tmp"));
        let context = create_test_context();

        let prompt = loader.load_reviewer_prompt(&context, 1);

        assert!(prompt.contains("owner/repo"));
        assert!(prompt.contains("PR #123"));
        assert!(prompt.contains("Add feature"));
        assert!(prompt.contains("This adds a new feature"));
        assert!(prompt.contains("+added line"));
        assert!(prompt.contains("iteration 1"));
    }

    #[test]
    fn test_load_reviewee_prompt() {
        let config = AiConfig::default();
        let loader = PromptLoader::new(&config, Path::new("/tmp"));
        let context = create_test_context();

        let review = ReviewerOutput {
            action: ReviewAction::RequestChanges,
            summary: "Please fix the issues".to_string(),
            comments: vec![ReviewComment {
                path: "src/main.rs".to_string(),
                line: 10,
                body: "Missing error handling".to_string(),
                severity: CommentSeverity::Major,
            }],
            blocking_issues: vec!["Fix error handling".to_string()],
        };

        let prompt = loader.load_reviewee_prompt(&context, &review, 1);

        assert!(prompt.contains("owner/repo"));
        assert!(prompt.contains("PR #123"));
        assert!(prompt.contains("Please fix the issues"));
        assert!(prompt.contains("RequestChanges"));
        assert!(prompt.contains("src/main.rs:10"));
        assert!(prompt.contains("Missing error handling"));
        assert!(prompt.contains("Fix error handling"));
    }

    #[test]
    fn test_load_reviewee_prompt_with_external_comments() {
        let config = AiConfig::default();
        let loader = PromptLoader::new(&config, Path::new("/tmp"));
        let mut context = create_test_context();
        context.external_comments = vec![
            ExternalComment {
                source: "copilot[bot]".to_string(),
                path: Some("src/main.rs".to_string()),
                line: Some(42),
                body: "Consider using a more descriptive variable name".to_string(),
            },
            ExternalComment {
                source: "coderabbitai[bot]".to_string(),
                path: None,
                line: None,
                body: "Overall code quality looks good!".to_string(),
            },
        ];

        let review = ReviewerOutput {
            action: ReviewAction::RequestChanges,
            summary: "Please fix the issues".to_string(),
            comments: vec![],
            blocking_issues: vec![],
        };

        let prompt = loader.load_reviewee_prompt(&context, &review, 1);

        assert!(prompt.contains("External Tool Feedback"));
        assert!(prompt.contains("copilot[bot]"));
        assert!(prompt.contains("coderabbitai[bot]"));
        assert!(prompt.contains("src/main.rs:42"));
    }

    #[test]
    fn test_load_rereview_prompt() {
        let config = AiConfig::default();
        let loader = PromptLoader::new(&config, Path::new("/tmp"));
        let context = create_test_context();

        let prompt = loader.load_rereview_prompt(
            &context,
            2,
            "Fixed error handling",
            "+new code\n-old code",
        );

        assert!(prompt.contains("owner/repo"));
        assert!(prompt.contains("PR #123"));
        assert!(prompt.contains("Iteration 2"));
        assert!(prompt.contains("Fixed error handling"));
        assert!(prompt.contains("+new code"));
    }

    /// Create a PromptLoader that always uses default templates (ignoring custom prompt dir)
    fn create_default_loader() -> PromptLoader {
        PromptLoader {
            prompt_dir: None,
            local_prompts_dir: None,
            global_prompts_dir: None,
        }
    }

    #[test]
    fn test_load_reviewee_prompt_local_mode_git_operations() {
        let loader = create_default_loader();
        let mut context = create_test_context();
        context.local_mode = true;

        let review = ReviewerOutput {
            action: ReviewAction::RequestChanges,
            summary: "Fix issues".to_string(),
            comments: vec![],
            blocking_issues: vec![],
        };

        let prompt = loader.load_reviewee_prompt(&context, &review, 1);

        // Local mode: should contain prohibition
        assert!(prompt.contains("LOCAL-ONLY session"));
        assert!(prompt.contains("Do NOT run any git write commands"));
        // Should NOT contain normal git operations
        assert!(!prompt.contains("you MUST commit your changes locally"));
        assert!(!prompt.contains("git add <files>"));
    }

    #[test]
    fn test_load_reviewee_prompt_normal_mode_git_operations() {
        let loader = create_default_loader();
        let context = create_test_context(); // local_mode = false

        let review = ReviewerOutput {
            action: ReviewAction::RequestChanges,
            summary: "Fix issues".to_string(),
            comments: vec![],
            blocking_issues: vec![],
        };

        let prompt = loader.load_reviewee_prompt(&context, &review, 1);

        // Normal mode: should contain commit instructions
        assert!(prompt.contains("you MUST commit your changes locally"));
        assert!(prompt.contains("git add <files>"));
        // Should NOT contain local mode prohibition
        assert!(!prompt.contains("LOCAL-ONLY session"));
    }

    #[test]
    fn test_resolve_source_embedded_when_no_dirs() {
        let loader = create_default_loader();
        let source = loader.resolve_source("reviewer.md");
        assert_eq!(source, PromptSource::Embedded);
    }

    #[test]
    fn test_resolve_source_local_priority() {
        let dir = tempfile::tempdir().unwrap();
        let local_dir = dir.path().join("local_prompts");
        std::fs::create_dir_all(&local_dir).unwrap();
        std::fs::write(local_dir.join("reviewer.md"), "local prompt").unwrap();

        let loader = PromptLoader {
            prompt_dir: None,
            local_prompts_dir: Some(local_dir.clone()),
            global_prompts_dir: None,
        };
        let source = loader.resolve_source("reviewer.md");
        assert_eq!(source, PromptSource::Local(local_dir.join("reviewer.md")));
    }

    #[test]
    fn test_resolve_source_prompt_dir_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let prompt_dir = dir.path().join("prompt_dir");
        std::fs::create_dir_all(&prompt_dir).unwrap();
        std::fs::write(prompt_dir.join("reviewer.md"), "prompt dir").unwrap();

        let loader = PromptLoader {
            prompt_dir: Some(prompt_dir.clone()),
            local_prompts_dir: None,
            global_prompts_dir: None,
        };
        let source = loader.resolve_source("reviewer.md");
        assert_eq!(
            source,
            PromptSource::PromptDir(prompt_dir.join("reviewer.md"))
        );
    }

    #[test]
    fn test_resolve_source_global_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let global_dir = dir.path().join("global_prompts");
        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::write(global_dir.join("reviewer.md"), "global prompt").unwrap();

        let loader = PromptLoader {
            prompt_dir: None,
            local_prompts_dir: None,
            global_prompts_dir: Some(global_dir.clone()),
        };
        let source = loader.resolve_source("reviewer.md");
        assert_eq!(
            source,
            PromptSource::Global(global_dir.join("reviewer.md"))
        );
    }

    #[test]
    fn test_resolve_all_sources_returns_three_files() {
        let loader = create_default_loader();
        let sources = loader.resolve_all_sources();
        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].0, "reviewer.md");
        assert_eq!(sources[1].0, "reviewee.md");
        assert_eq!(sources[2].0, "rereview.md");
        // All should be Embedded since no dirs configured
        for (_, source) in &sources {
            assert_eq!(*source, PromptSource::Embedded);
        }
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hello", 2), "he");
        assert_eq!(truncate("hello", 3), "hel");
    }
}
