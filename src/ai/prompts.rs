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
