use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::config::AiConfig;

use super::adapter::{Context, ReviewAction, ReviewerOutput};

/// Default prompt templates embedded in the binary
mod defaults {
    pub const REVIEWER: &str = include_str!("defaults/reviewer.md");
    pub const REVIEWEE: &str = include_str!("defaults/reviewee.md");
    pub const REREVIEW: &str = include_str!("defaults/rereview.md");
}

/// Prompt loader that reads templates from files or uses defaults
pub struct PromptLoader {
    prompt_dir: Option<PathBuf>,
}

impl PromptLoader {
    /// Create a new PromptLoader with the given config
    pub fn new(config: &AiConfig) -> Self {
        let prompt_dir = config.prompt_dir.as_ref().map(PathBuf::from).or_else(|| {
            // Default: ~/.config/octorus/prompts/
            xdg::BaseDirectories::with_prefix("octorus")
                .ok()
                .map(|dirs| dirs.get_config_home().join("prompts"))
        });

        Self { prompt_dir }
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

    /// Load a template from file or return default
    fn load_template(&self, filename: &str, default: &str) -> String {
        if let Some(ref dir) = self.prompt_dir {
            let path = dir.join(filename);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    return content;
                }
            }
        }
        default.to_string()
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
        let loader = PromptLoader::new(&config);
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
        let loader = PromptLoader::new(&config);
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
        let loader = PromptLoader::new(&config);
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
        let loader = PromptLoader::new(&config);
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

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hello", 2), "he");
        assert_eq!(truncate("hello", 3), "hel");
    }
}
