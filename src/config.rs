use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use xdg::BaseDirectories;

/// Additional tools that can be allowed for AI Rally agents (Claude adapter only).
/// Dangerous tools (Write, Edit, Bash) are intentionally excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AllowedTool {
    // Claude Code tools
    /// Execute Claude Code skills
    Skill,
    /// Fetch URL content
    WebFetch,
    /// Web search
    WebSearch,
    // Git operations (disabled by default)
    /// git push to remote (reviewee only)
    GitPush,
}

impl AllowedTool {
    /// Convert to Claude CLI's --allowedTools format string.
    /// GitPush returns a Bash pattern.
    pub fn as_tool_pattern(&self) -> &'static str {
        match self {
            AllowedTool::Skill => "Skill",
            AllowedTool::WebFetch => "WebFetch",
            AllowedTool::WebSearch => "WebSearch",
            AllowedTool::GitPush => "Bash(git push:*)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub editor: String,
    pub diff: DiffConfig,
    pub keybindings: KeybindingsConfig,
    pub ai: AiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub reviewer: String,
    pub reviewee: String,
    pub max_iterations: u32,
    pub timeout_secs: u64,
    /// Custom prompt directory (default: ~/.config/octorus/prompts/)
    pub prompt_dir: Option<String>,
    /// Additional tools for reviewer (Claude adapter only).
    /// Available: Skill, WebFetch, WebSearch
    #[serde(default)]
    pub reviewer_additional_tools: Vec<AllowedTool>,
    /// Additional tools for reviewee (Claude adapter only).
    /// Available: Skill, WebFetch, WebSearch, GitPush
    #[serde(default)]
    pub reviewee_additional_tools: Vec<AllowedTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffConfig {
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub approve: char,
    pub request_changes: char,
    pub comment: char,
    pub suggestion: char,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            editor: "vi".to_owned(),
            diff: DiffConfig::default(),
            keybindings: KeybindingsConfig::default(),
            ai: AiConfig::default(),
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            reviewer: "claude".to_owned(),
            reviewee: "claude".to_owned(),
            max_iterations: 10,
            timeout_secs: 600,
            prompt_dir: None,
            reviewer_additional_tools: Vec::new(),
            reviewee_additional_tools: Vec::new(),
        }
    }
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            theme: "base16-ocean.dark".to_owned(),
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            approve: 'a',
            request_changes: 'r',
            comment: 'c',
            suggestion: 's',
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = fs::read_to_string(&config_path).context("Failed to read config file")?;
            toml::from_str(&content).context("Failed to parse config file")
        } else {
            Ok(Self::default())
        }
    }

    fn config_path() -> PathBuf {
        BaseDirectories::with_prefix("octorus")
            .map(|dirs| dirs.get_config_home().join("config.toml"))
            .unwrap_or_else(|_| PathBuf::from("config.toml"))
    }
}
