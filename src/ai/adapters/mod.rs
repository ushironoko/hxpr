mod claude;
mod codex;

pub use claude::ClaudeAdapter;
pub use codex::CodexAdapter;

use anyhow::{anyhow, Result};

use super::adapter::{AgentAdapter, SupportedAgent};
use crate::config::AiConfig;

/// Create an adapter from agent name.
///
/// # Arguments
/// * `name` - Agent name ("claude" or "codex")
/// * `config` - AI configuration (used by Claude adapter for additional tools, ignored by Codex)
pub fn create_adapter(name: &str, config: &AiConfig) -> Result<Box<dyn AgentAdapter>> {
    let agent = SupportedAgent::from_name(name)
        .ok_or_else(|| anyhow!("Unsupported agent: {}. Supported: claude, codex", name))?;

    match agent {
        // Claude adapter uses config for additional tools
        SupportedAgent::Claude => Ok(Box::new(ClaudeAdapter::new(config))),
        // Codex adapter does not support fine-grained tool control
        SupportedAgent::Codex => Ok(Box::new(CodexAdapter::new())),
        // SupportedAgent::Gemini => Ok(Box::new(GeminiAdapter::new())),
    }
}
