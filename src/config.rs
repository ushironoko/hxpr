use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use xdg::BaseDirectories;

use crate::keybinding::{KeyBinding, KeySequence, NamedKey};

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
    /// Use Claude Code's --allowedTools format (e.g., "Skill", "Bash(git push:*)").
    #[serde(default)]
    pub reviewer_additional_tools: Vec<String>,
    /// Additional tools for reviewee (Claude adapter only).
    /// Use Claude Code's --allowedTools format (e.g., "Skill", "Bash(git push:*)").
    #[serde(default)]
    pub reviewee_additional_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffConfig {
    pub theme: String,
}

/// Configurable keybindings
///
/// Supports three formats in TOML:
/// - Simple string: `move_down = "j"`
/// - Object with modifiers: `page_down = { key = "d", ctrl = true }`
/// - Array for sequences: `jump_to_first = ["g", "g"]`
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    // Navigation
    pub move_down: KeySequence,
    pub move_up: KeySequence,
    pub move_left: KeySequence,
    pub move_right: KeySequence,
    pub page_down: KeySequence,
    pub page_up: KeySequence,
    pub jump_to_first: KeySequence,
    pub jump_to_last: KeySequence,
    pub jump_back: KeySequence,
    pub next_comment: KeySequence,
    pub prev_comment: KeySequence,

    // Actions
    pub approve: KeySequence,
    pub request_changes: KeySequence,
    pub comment: KeySequence,
    pub suggestion: KeySequence,
    pub reply: KeySequence,
    pub refresh: KeySequence,
    pub submit: KeySequence,

    // Mode switching
    pub quit: KeySequence,
    pub help: KeySequence,
    pub comment_list: KeySequence,
    pub ai_rally: KeySequence,
    pub open_panel: KeySequence,

    // Diff operations
    pub go_to_definition: KeySequence,
    pub go_to_file: KeySequence,
    pub open_in_browser: KeySequence,
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
            // Navigation
            move_down: KeySequence::single(KeyBinding::char('j')),
            move_up: KeySequence::single(KeyBinding::char('k')),
            move_left: KeySequence::single(KeyBinding::char('h')),
            move_right: KeySequence::single(KeyBinding::char('l')),
            page_down: KeySequence::single(KeyBinding::ctrl('d')),
            page_up: KeySequence::single(KeyBinding::ctrl('u')),
            jump_to_first: KeySequence::double(KeyBinding::char('g'), KeyBinding::char('g')),
            jump_to_last: KeySequence::single(KeyBinding::char('G')),
            jump_back: KeySequence::single(KeyBinding::ctrl('o')),
            next_comment: KeySequence::single(KeyBinding::char('n')),
            prev_comment: KeySequence::single(KeyBinding::char('N')),

            // Actions
            approve: KeySequence::single(KeyBinding::char('a')),
            request_changes: KeySequence::single(KeyBinding::char('r')),
            comment: KeySequence::single(KeyBinding::char('c')),
            suggestion: KeySequence::single(KeyBinding::char('s')),
            reply: KeySequence::single(KeyBinding::char('r')),
            refresh: KeySequence::single(KeyBinding::char('R')),
            submit: KeySequence::single(KeyBinding::ctrl('s')),

            // Mode switching
            quit: KeySequence::single(KeyBinding::char('q')),
            help: KeySequence::single(KeyBinding::char('?')),
            comment_list: KeySequence::single(KeyBinding::char('C')),
            ai_rally: KeySequence::single(KeyBinding::char('A')),
            open_panel: KeySequence::single(KeyBinding::named(NamedKey::Enter)),

            // Diff operations
            go_to_definition: KeySequence::double(KeyBinding::char('g'), KeyBinding::char('d')),
            go_to_file: KeySequence::double(KeyBinding::char('g'), KeyBinding::char('f')),
            open_in_browser: KeySequence::single(KeyBinding::char('O')),
        }
    }
}

impl KeybindingsConfig {
    /// Validate keybindings for conflicts
    ///
    /// Detects:
    /// - Single keys that conflict with sequence prefixes
    /// - Duplicate keybindings for different actions
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut single_keys: HashMap<KeyBinding, &str> = HashMap::new();
        let mut sequence_prefixes: HashMap<KeyBinding, &str> = HashMap::new();

        // Collect all keybindings
        let bindings: Vec<(&str, &KeySequence)> = vec![
            ("move_down", &self.move_down),
            ("move_up", &self.move_up),
            ("move_left", &self.move_left),
            ("move_right", &self.move_right),
            ("page_down", &self.page_down),
            ("page_up", &self.page_up),
            ("jump_to_first", &self.jump_to_first),
            ("jump_to_last", &self.jump_to_last),
            ("jump_back", &self.jump_back),
            ("next_comment", &self.next_comment),
            ("prev_comment", &self.prev_comment),
            ("approve", &self.approve),
            ("request_changes", &self.request_changes),
            ("comment", &self.comment),
            ("suggestion", &self.suggestion),
            ("reply", &self.reply),
            ("refresh", &self.refresh),
            ("submit", &self.submit),
            ("quit", &self.quit),
            ("help", &self.help),
            ("comment_list", &self.comment_list),
            ("ai_rally", &self.ai_rally),
            ("open_panel", &self.open_panel),
            ("go_to_definition", &self.go_to_definition),
            ("go_to_file", &self.go_to_file),
            ("open_in_browser", &self.open_in_browser),
        ];

        for (name, seq) in &bindings {
            if seq.0.is_empty() {
                errors.push(format!("keybinding '{}' is empty", name));
                continue;
            }

            if seq.is_single() {
                let key = seq.0[0];
                if let Some(existing) = single_keys.get(&key) {
                    // Allow same key for different contexts (e.g., 'r' for reply and request_changes)
                    // This is intentional - context determines which action is triggered
                    if !is_context_compatible(name, existing) {
                        errors.push(format!(
                            "duplicate keybinding: '{}' and '{}' both use {}",
                            name,
                            existing,
                            key.display()
                        ));
                    }
                } else {
                    single_keys.insert(key, name);
                }
            } else {
                // For sequences, track the first key as a prefix
                if let Some(first) = seq.first() {
                    sequence_prefixes.insert(*first, name);
                }
            }
        }

        // Check for conflicts between single keys and sequence prefixes
        for (key, single_name) in &single_keys {
            if let Some(seq_name) = sequence_prefixes.get(key) {
                // Only warn if they're in the same context
                if !is_context_compatible(single_name, seq_name) {
                    errors.push(format!(
                        "keybinding conflict: '{}' ({}) conflicts with sequence prefix for '{}' ({})",
                        single_name,
                        key.display(),
                        seq_name,
                        key.display()
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Check if two keybindings are in compatible contexts
/// (i.e., they won't conflict because they're used in different views)
fn is_context_compatible(name1: &str, name2: &str) -> bool {
    // These keybindings are used in different contexts:
    // - 'r' is used for 'reply' in comment panel and 'request_changes' in file list
    //
    // NOTE: 'comment' and 'suggestion' are NOT compatible - both are active in diff view
    // and comment panel contexts, so they must have different bindings.
    let context_groups: &[&[&str]] = &[&["reply", "request_changes"]];

    for group in context_groups {
        if group.contains(&name1) && group.contains(&name2) {
            return true;
        }
    }

    false
}

// Custom Serialize for KeybindingsConfig to maintain backwards compatibility
impl Serialize for KeybindingsConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        // For serialization, we output a simplified format
        let mut map = serializer.serialize_map(None)?;

        // Helper to serialize a KeySequence
        fn seq_to_value(seq: &KeySequence) -> toml::Value {
            if seq.is_single() {
                toml::Value::String(seq.display())
            } else {
                toml::Value::Array(
                    seq.0
                        .iter()
                        .map(|k| toml::Value::String(k.display()))
                        .collect(),
                )
            }
        }

        map.serialize_entry("move_down", &seq_to_value(&self.move_down))?;
        map.serialize_entry("move_up", &seq_to_value(&self.move_up))?;
        map.serialize_entry("move_left", &seq_to_value(&self.move_left))?;
        map.serialize_entry("move_right", &seq_to_value(&self.move_right))?;
        map.serialize_entry("page_down", &seq_to_value(&self.page_down))?;
        map.serialize_entry("page_up", &seq_to_value(&self.page_up))?;
        map.serialize_entry("jump_to_first", &seq_to_value(&self.jump_to_first))?;
        map.serialize_entry("jump_to_last", &seq_to_value(&self.jump_to_last))?;
        map.serialize_entry("jump_back", &seq_to_value(&self.jump_back))?;
        map.serialize_entry("next_comment", &seq_to_value(&self.next_comment))?;
        map.serialize_entry("prev_comment", &seq_to_value(&self.prev_comment))?;
        map.serialize_entry("approve", &seq_to_value(&self.approve))?;
        map.serialize_entry("request_changes", &seq_to_value(&self.request_changes))?;
        map.serialize_entry("comment", &seq_to_value(&self.comment))?;
        map.serialize_entry("suggestion", &seq_to_value(&self.suggestion))?;
        map.serialize_entry("reply", &seq_to_value(&self.reply))?;
        map.serialize_entry("refresh", &seq_to_value(&self.refresh))?;
        map.serialize_entry("submit", &seq_to_value(&self.submit))?;
        map.serialize_entry("quit", &seq_to_value(&self.quit))?;
        map.serialize_entry("help", &seq_to_value(&self.help))?;
        map.serialize_entry("comment_list", &seq_to_value(&self.comment_list))?;
        map.serialize_entry("ai_rally", &seq_to_value(&self.ai_rally))?;
        map.serialize_entry("open_panel", &seq_to_value(&self.open_panel))?;
        map.serialize_entry("go_to_definition", &seq_to_value(&self.go_to_definition))?;
        map.serialize_entry("go_to_file", &seq_to_value(&self.go_to_file))?;
        map.serialize_entry("open_in_browser", &seq_to_value(&self.open_in_browser))?;

        map.end()
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        let config = if config_path.exists() {
            let content = fs::read_to_string(&config_path).context("Failed to read config file")?;
            toml::from_str(&content).context("Failed to parse config file")?
        } else {
            Self::default()
        };

        // Validate keybindings and warn on conflicts
        if let Err(errors) = config.keybindings.validate() {
            for error in errors {
                eprintln!("Warning: {}", error);
            }
        }

        Ok(config)
    }

    fn config_path() -> PathBuf {
        BaseDirectories::with_prefix("octorus")
            .map(|dirs| dirs.get_config_home().join("config.toml"))
            .unwrap_or_else(|_| PathBuf::from("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_json_snapshot;

    #[test]
    fn test_default_keybindings() {
        let config = KeybindingsConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_parse_simple_keybinding() {
        let toml_str = r#"
            [keybindings]
            move_down = "n"
            move_up = "e"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.keybindings.move_down.display(), "n");
        assert_eq!(config.keybindings.move_up.display(), "e");
    }

    #[test]
    fn test_parse_modifier_keybinding() {
        let toml_str = r#"
            [keybindings]
            page_down = { key = "f", ctrl = true }
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.keybindings.page_down.display(), "Ctrl-f");
    }

    #[test]
    fn test_parse_sequence_keybinding() {
        let toml_str = r#"
            [keybindings]
            jump_to_first = ["g", "g"]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.keybindings.jump_to_first.display(), "gg");
    }

    #[test]
    fn test_backwards_compatible_defaults() {
        // Empty config should use all defaults
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.keybindings.approve.display(), "a");
        assert_eq!(config.keybindings.request_changes.display(), "r");
    }

    #[test]
    fn test_parse_ai_config_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_json_snapshot!(config.ai, @r#"
        {
          "reviewer": "claude",
          "reviewee": "claude",
          "max_iterations": 10,
          "timeout_secs": 600,
          "prompt_dir": null,
          "reviewer_additional_tools": [],
          "reviewee_additional_tools": []
        }
        "#);
    }

    #[test]
    fn test_parse_ai_config_custom() {
        let toml_str = r#"
            [ai]
            reviewer = "codex"
            reviewee = "claude"
            max_iterations = 5
            timeout_secs = 300
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_json_snapshot!(config.ai, @r#"
        {
          "reviewer": "codex",
          "reviewee": "claude",
          "max_iterations": 5,
          "timeout_secs": 300,
          "prompt_dir": null,
          "reviewer_additional_tools": [],
          "reviewee_additional_tools": []
        }
        "#);
    }

    #[test]
    fn test_parse_ai_config_with_additional_tools() {
        let toml_str = r#"
            [ai]
            reviewer_additional_tools = ["Skill", "WebSearch"]
            reviewee_additional_tools = ["Bash(git push:*)"]
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_json_snapshot!(config.ai, @r#"
        {
          "reviewer": "claude",
          "reviewee": "claude",
          "max_iterations": 10,
          "timeout_secs": 600,
          "prompt_dir": null,
          "reviewer_additional_tools": [
            "Skill",
            "WebSearch"
          ],
          "reviewee_additional_tools": [
            "Bash(git push:*)"
          ]
        }
        "#);
    }
}
