use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub editor: String,
    pub diff: DiffConfig,
    pub keybindings: KeybindingsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffConfig {
    pub renderer: String,
    pub side_by_side: bool,
    pub line_numbers: bool,
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
            editor: "hx".to_owned(),
            diff: DiffConfig::default(),
            keybindings: KeybindingsConfig::default(),
        }
    }
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            renderer: "delta".to_owned(),
            side_by_side: true,
            line_numbers: true,
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
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("octorus")
            .join("config.toml")
    }
}
