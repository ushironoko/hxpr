mod claude;
mod codex;

pub use claude::ClaudeAdapter;
pub use codex::CodexAdapter;

use anyhow::{anyhow, Result};

use super::adapter::{AgentAdapter, SupportedAgent};

/// Create an adapter from agent name
pub fn create_adapter(name: &str) -> Result<Box<dyn AgentAdapter>> {
    let agent = SupportedAgent::from_name(name)
        .ok_or_else(|| anyhow!("Unsupported agent: {}. Supported: claude, codex", name))?;

    match agent {
        SupportedAgent::Claude => Ok(Box::new(ClaudeAdapter::new())),
        SupportedAgent::Codex => Ok(Box::new(CodexAdapter::new())),
        // SupportedAgent::Gemini => Ok(Box::new(GeminiAdapter::new())),
    }
}
