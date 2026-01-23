use crate::ai::adapter::{Context, ReviewerOutput};

/// Build the initial reviewer prompt
///
/// If `custom_prompt` is provided, it will be prepended to the default prompt.
pub fn build_reviewer_prompt(
    context: &Context,
    iteration: u32,
    custom_prompt: Option<&str>,
) -> String {
    let pr_body = context
        .pr_body
        .as_deref()
        .unwrap_or("(No description provided)");

    let custom_section = custom_prompt
        .map(|p| format!("## Custom Instructions\n\n{}\n\n", p))
        .unwrap_or_default();

    format!(
        r#"{custom_section}You are a code reviewer for a GitHub Pull Request.

## Context

Repository: {repo}
PR #{pr_number}: {pr_title}

### PR Description
{pr_body}

### Diff
```diff
{diff}
```

## Your Task

This is iteration {iteration} of the review process.

1. Carefully review the changes in the diff
2. Check for:
   - Code quality issues
   - Potential bugs
   - Security vulnerabilities
   - Performance concerns
   - Style and consistency issues
   - Missing tests or documentation

3. Provide your review decision:
   - "approve" if the changes are good to merge
   - "request_changes" if there are issues that must be fixed
   - "comment" if you have suggestions but they're not blocking

4. List any blocking issues that must be resolved before approval

## Output Format

You MUST respond with a JSON object matching the schema provided.
Be specific in your comments with file paths and line numbers."#,
        custom_section = custom_section,
        repo = context.repo,
        pr_number = context.pr_number,
        pr_title = context.pr_title,
        pr_body = pr_body,
        diff = context.diff,
        iteration = iteration,
    )
}

/// Build the reviewee prompt based on review feedback
///
/// If `custom_prompt` is provided, it will be prepended to the default prompt.
pub fn build_reviewee_prompt(
    context: &Context,
    review: &ReviewerOutput,
    iteration: u32,
    custom_prompt: Option<&str>,
) -> String {
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

    let custom_section = custom_prompt
        .map(|p| format!("## Custom Instructions\n\n{}\n\n", p))
        .unwrap_or_default();

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

    format!(
        r#"{custom_section}You are a developer fixing code based on review feedback.

## Context

Repository: {repo}
PR #{pr_number}: {pr_title}

## Review Feedback (Iteration {iteration})

### Summary
{summary}

### Review Action: {action:?}

### Comments
{comments}

### Blocking Issues
{blocking}
{external_section}
## Git Operations

After making changes, you MUST commit locally (do NOT push):

1. Check status: `git status`
2. Stage files: `git add <files>`
3. Commit: `git commit -m "fix: <description>"`

CRITICAL RULES:
- Do NOT use `git push` - pushing is done manually by the user after review
- NEVER use `git reset --hard` - this destroys work
- NEVER use `git clean -fd` - this deletes untracked files permanently
- Use `gh` commands for GitHub API operations (viewing PR info, comments, etc.)

## Your Task

1. Address each blocking issue and review comment
2. Make the necessary code changes
3. Commit your changes locally (do NOT push)
4. If something is unclear, set status to "needs_clarification" and ask a question
5. If you need permission for a significant change, set status to "needs_permission"

## Output Format

You MUST respond with a JSON object matching the schema provided.
List all files you modified in the "files_modified" array."#,
        custom_section = custom_section,
        repo = context.repo,
        pr_number = context.pr_number,
        pr_title = context.pr_title,
        iteration = iteration,
        summary = review.summary,
        action = review.action,
        comments = comments_text,
        blocking = blocking_text,
        external_section = external_section,
    )
}

/// Truncate a string to a maximum length (UTF-8 safe)
fn truncate(s: &str, max_len: usize) -> String {
    let s = s.trim();
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        // When max_len is too small for ellipsis, just take max_len chars without ellipsis
        s.chars().take(max_len).collect()
    } else {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    }
}

/// Build a prompt for asking the reviewer a clarification question
#[allow(dead_code)]
pub fn build_clarification_prompt(question: &str) -> String {
    format!(
        r#"The developer has a question about your review feedback:

## Question
{question}

Please provide a clear answer to help them proceed with the fixes.
After answering, provide an updated review if needed."#,
        question = question,
    )
}

/// Build a prompt for continuing after permission is granted
#[allow(dead_code)]
pub fn build_permission_granted_prompt(action: &str) -> String {
    format!(
        r#"Permission has been granted for the following action:

{action}

Please proceed with the implementation."#,
        action = action,
    )
}

/// Build a re-review prompt after fixes
pub fn build_rereview_prompt(
    context: &Context,
    iteration: u32,
    changes_summary: &str,
    updated_diff: &str,
) -> String {
    format!(
        r#"The developer has made changes based on your review feedback.

## Context

Repository: {repo}
PR #{pr_number}: {pr_title}

## Changes Made (Iteration {iteration})
{changes_summary}

## Updated Diff (Current State)
```diff
{updated_diff}
```

## Your Task

1. Re-review the changes in the updated diff
2. Check if the blocking issues have been addressed
3. Look for any new issues introduced by the fixes
4. Decide if the PR is now ready to merge

## Output Format

You MUST respond with a JSON object matching the schema provided."#,
        repo = context.repo,
        pr_number = context.pr_number,
        pr_title = context.pr_title,
        iteration = iteration,
        changes_summary = changes_summary,
        updated_diff = updated_diff,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::{CommentSeverity, ExternalComment, ReviewAction, ReviewComment};

    #[test]
    fn test_build_reviewer_prompt() {
        let context = Context {
            repo: "owner/repo".to_string(),
            pr_number: 123,
            pr_title: "Add feature".to_string(),
            pr_body: Some("This adds a new feature".to_string()),
            diff: "+added line\n-removed line".to_string(),
            working_dir: None,
            head_sha: "abc123".to_string(),
            external_comments: Vec::new(),
        };

        let prompt = build_reviewer_prompt(&context, 1, None);
        assert!(prompt.contains("owner/repo"));
        assert!(prompt.contains("PR #123"));
        assert!(prompt.contains("Add feature"));
        assert!(prompt.contains("iteration 1"));

        // Test with custom prompt
        let prompt_with_custom =
            build_reviewer_prompt(&context, 1, Some("Focus on security issues"));
        assert!(prompt_with_custom.contains("Focus on security issues"));
        assert!(prompt_with_custom.contains("Custom Instructions"));
    }

    #[test]
    fn test_build_reviewee_prompt() {
        let context = Context {
            repo: "owner/repo".to_string(),
            pr_number: 123,
            pr_title: "Add feature".to_string(),
            pr_body: None,
            diff: "".to_string(),
            working_dir: None,
            head_sha: "abc123".to_string(),
            external_comments: Vec::new(),
        };

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

        let prompt = build_reviewee_prompt(&context, &review, 1, None);
        assert!(prompt.contains("src/main.rs:10"));
        assert!(prompt.contains("Missing error handling"));
        assert!(prompt.contains("Fix error handling"));

        // Test with custom prompt
        let prompt_with_custom = build_reviewee_prompt(
            &context,
            &review,
            1,
            Some("Run cargo fmt before committing"),
        );
        assert!(prompt_with_custom.contains("Run cargo fmt before committing"));
        assert!(prompt_with_custom.contains("Custom Instructions"));
    }

    #[test]
    fn test_build_reviewee_prompt_with_external_comments() {
        let context = Context {
            repo: "owner/repo".to_string(),
            pr_number: 123,
            pr_title: "Add feature".to_string(),
            pr_body: None,
            diff: "".to_string(),
            working_dir: None,
            head_sha: "abc123".to_string(),
            external_comments: vec![
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
            ],
        };

        let review = ReviewerOutput {
            action: ReviewAction::RequestChanges,
            summary: "Please fix the issues".to_string(),
            comments: vec![],
            blocking_issues: vec![],
        };

        let prompt = build_reviewee_prompt(&context, &review, 1, None);

        // Check external comments section exists
        assert!(prompt.contains("External Tool Feedback"));
        assert!(prompt.contains("copilot[bot]"));
        assert!(prompt.contains("coderabbitai[bot]"));
        assert!(prompt.contains("src/main.rs:42"));
        assert!(prompt.contains("Consider using a more descriptive variable name"));
        assert!(prompt.contains("general")); // For the comment without path

        // Check git instructions are present
        assert!(prompt.contains("Do NOT use `git push`"));
    }

    #[test]
    fn test_truncate() {
        // Short string - no truncation
        assert_eq!(truncate("hello", 10), "hello");

        // Exact length - no truncation
        assert_eq!(truncate("hello", 5), "hello");

        // Long string - truncated
        let long_str = "This is a very long string that should be truncated";
        let truncated = truncate(long_str, 20);
        // Use char count for consistency with the truncate function which operates on characters
        assert!(truncated.chars().count() <= 20);
        assert!(truncated.ends_with("..."));

        // Unicode handling
        let unicode = "こんにちは世界";
        let truncated_unicode = truncate(unicode, 5);
        assert!(truncated_unicode.chars().count() <= 5);

        // Edge case: max_len <= 3 (where ellipsis would not fit)
        // Should return first max_len chars without ellipsis
        assert_eq!(truncate("hello", 2), "he");
        assert_eq!(truncate("hello", 3), "hel");
        assert_eq!(truncate("hello", 1), "h");
        assert_eq!(truncate("hello", 0), "");

        // Edge case: max_len = 4 (just enough for 1 char + ellipsis)
        assert_eq!(truncate("hello", 4), "h...");
    }
}
