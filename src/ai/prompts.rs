// For Clarification/Permission flow (not yet implemented)
// See CLAUDE.md "Known Limitations"
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

// For Clarification/Permission flow (not yet implemented)
// See CLAUDE.md "Known Limitations"
#[allow(dead_code)]
pub fn build_permission_granted_prompt(action: &str) -> String {
    format!(
        r#"Permission has been granted for the following action:

{action}

Please proceed with the implementation."#,
        action = action,
    )
}

/// Prompt for when permission is denied
pub fn build_permission_denied_prompt(action: &str, reason: &str) -> String {
    format!(
        r#"The user has DENIED permission for the following action:

## Denied Action
{action}

## Original Reason
{reason}

## Your Task

You CANNOT perform this action. Instead:
1. Find an alternative approach that doesn't require this permission
2. If no alternative exists, document what cannot be done and proceed with other fixes
3. If the fix is completely blocked, set status to "completed" with a summary explaining what couldn't be done

Do NOT ask for this permission again. Work within your current constraints."#,
        action = action,
        reason = reason,
    )
}

/// Prompt for when clarification is skipped
pub fn build_clarification_skipped_prompt(question: &str) -> String {
    format!(
        r#"The user chose NOT to answer your clarification question:

## Unanswered Question
{question}

## Your Task

Proceed WITHOUT this clarification. Use your best judgment based on:
1. Common patterns and best practices
2. The context from the review feedback
3. Conservative assumptions (prefer safe, non-breaking changes)

If you're completely uncertain, make minimal changes and document your assumptions in the summary."#,
        question = question,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_build_clarification_prompt() {
        let result = build_clarification_prompt("How should error handling work?");
        assert_snapshot!(result, @r#"
        The developer has a question about your review feedback:

        ## Question
        How should error handling work?

        Please provide a clear answer to help them proceed with the fixes.
        After answering, provide an updated review if needed.
        "#);
    }

    #[test]
    fn test_build_permission_granted_prompt() {
        let result = build_permission_granted_prompt("run npm install");
        assert_snapshot!(result, @r#"
        Permission has been granted for the following action:

        run npm install

        Please proceed with the implementation.
        "#);
    }

    #[test]
    fn test_build_permission_denied_prompt() {
        let result = build_permission_denied_prompt("git push --force", "Need to update remote");
        assert_snapshot!(result, @r#"
        The user has DENIED permission for the following action:

        ## Denied Action
        git push --force

        ## Original Reason
        Need to update remote

        ## Your Task

        You CANNOT perform this action. Instead:
        1. Find an alternative approach that doesn't require this permission
        2. If no alternative exists, document what cannot be done and proceed with other fixes
        3. If the fix is completely blocked, set status to "completed" with a summary explaining what couldn't be done

        Do NOT ask for this permission again. Work within your current constraints.
        "#);
    }

    #[test]
    fn test_build_clarification_skipped_prompt() {
        let result = build_clarification_skipped_prompt("What database should we use?");
        assert_snapshot!(result, @r#"
        The user chose NOT to answer your clarification question:

        ## Unanswered Question
        What database should we use?

        ## Your Task

        Proceed WITHOUT this clarification. Use your best judgment based on:
        1. Common patterns and best practices
        2. The context from the review feedback
        3. Conservative assumptions (prefer safe, non-breaking changes)

        If you're completely uncertain, make minimal changes and document your assumptions in the summary.
        "#);
    }
}
