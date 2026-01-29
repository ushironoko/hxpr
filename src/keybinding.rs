//! Keybinding types and matching logic
//!
//! This module provides configurable keybinding support with:
//! - Single keys (e.g., "j", "k")
//! - Modifier keys (e.g., Ctrl+d, Ctrl+u)
//! - Two-key sequences (e.g., "gg", "gd")

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::time::{Duration, Instant};

/// Timeout for key sequences (500ms)
pub const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(500);

/// Named special keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Enter,
    Tab,
    Esc,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    BackTab,
}

impl NamedKey {
    /// Parse from string representation
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "enter" | "return" | "cr" => Some(NamedKey::Enter),
            "tab" => Some(NamedKey::Tab),
            "esc" | "escape" => Some(NamedKey::Esc),
            "backspace" | "bs" => Some(NamedKey::Backspace),
            "delete" | "del" => Some(NamedKey::Delete),
            "up" => Some(NamedKey::Up),
            "down" => Some(NamedKey::Down),
            "left" => Some(NamedKey::Left),
            "right" => Some(NamedKey::Right),
            "home" => Some(NamedKey::Home),
            "end" => Some(NamedKey::End),
            "pageup" | "pgup" => Some(NamedKey::PageUp),
            "pagedown" | "pgdn" => Some(NamedKey::PageDown),
            "backtab" | "shifttab" => Some(NamedKey::BackTab),
            _ => None,
        }
    }

    /// Convert to crossterm KeyCode
    pub fn to_keycode(self) -> KeyCode {
        match self {
            NamedKey::Enter => KeyCode::Enter,
            NamedKey::Tab => KeyCode::Tab,
            NamedKey::Esc => KeyCode::Esc,
            NamedKey::Backspace => KeyCode::Backspace,
            NamedKey::Delete => KeyCode::Delete,
            NamedKey::Up => KeyCode::Up,
            NamedKey::Down => KeyCode::Down,
            NamedKey::Left => KeyCode::Left,
            NamedKey::Right => KeyCode::Right,
            NamedKey::Home => KeyCode::Home,
            NamedKey::End => KeyCode::End,
            NamedKey::PageUp => KeyCode::PageUp,
            NamedKey::PageDown => KeyCode::PageDown,
            NamedKey::BackTab => KeyCode::BackTab,
        }
    }

    /// Display name for help screen
    pub fn display_name(&self) -> &'static str {
        match self {
            NamedKey::Enter => "Enter",
            NamedKey::Tab => "Tab",
            NamedKey::Esc => "Esc",
            NamedKey::Backspace => "Backspace",
            NamedKey::Delete => "Delete",
            NamedKey::Up => "Up",
            NamedKey::Down => "Down",
            NamedKey::Left => "Left",
            NamedKey::Right => "Right",
            NamedKey::Home => "Home",
            NamedKey::End => "End",
            NamedKey::PageUp => "PageUp",
            NamedKey::PageDown => "PageDown",
            NamedKey::BackTab => "Shift-Tab",
        }
    }
}

/// Key code representation (character or special key)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCodeConfig {
    Char(char),
    Named(NamedKey),
}

impl KeyCodeConfig {
    /// Convert to crossterm KeyCode
    pub fn to_keycode(self) -> KeyCode {
        match self {
            KeyCodeConfig::Char(c) => KeyCode::Char(c),
            KeyCodeConfig::Named(n) => n.to_keycode(),
        }
    }

    /// Display string for help screen
    pub fn display(&self) -> String {
        match self {
            KeyCodeConfig::Char(c) => c.to_string(),
            KeyCodeConfig::Named(n) => n.display_name().to_string(),
        }
    }
}

/// Modifier keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Modifiers {
    /// Create modifiers with only ctrl set
    pub fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Default::default()
        }
    }

    /// Create modifiers with only shift set
    pub fn shift() -> Self {
        Self {
            shift: true,
            ..Default::default()
        }
    }

    /// Check if no modifiers are set
    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.shift && !self.alt
    }

    /// Convert to crossterm KeyModifiers
    pub fn to_crossterm(&self) -> KeyModifiers {
        let mut mods = KeyModifiers::empty();
        if self.ctrl {
            mods |= KeyModifiers::CONTROL;
        }
        if self.shift {
            mods |= KeyModifiers::SHIFT;
        }
        if self.alt {
            mods |= KeyModifiers::ALT;
        }
        mods
    }

    /// Check if crossterm KeyModifiers match (ignoring extra modifiers from crossterm)
    pub fn matches(&self, key_mods: KeyModifiers) -> bool {
        let ctrl_match = !self.ctrl || key_mods.contains(KeyModifiers::CONTROL);
        let shift_match = !self.shift || key_mods.contains(KeyModifiers::SHIFT);
        let alt_match = !self.alt || key_mods.contains(KeyModifiers::ALT);

        // For characters without explicit modifiers, we need stricter matching
        if self.is_empty() {
            // No modifiers expected, but allow SHIFT for uppercase chars
            !key_mods.contains(KeyModifiers::CONTROL) && !key_mods.contains(KeyModifiers::ALT)
        } else {
            ctrl_match && shift_match && alt_match
        }
    }
}

/// Single keybinding (key + modifiers)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub code: KeyCodeConfig,
    pub modifiers: Modifiers,
}

impl KeyBinding {
    /// Create a simple character keybinding
    pub fn char(c: char) -> Self {
        // Uppercase letters implicitly have shift
        let (code, modifiers) = if c.is_ascii_uppercase() {
            (
                KeyCodeConfig::Char(c.to_ascii_lowercase()),
                Modifiers::shift(),
            )
        } else {
            (KeyCodeConfig::Char(c), Modifiers::default())
        };
        Self { code, modifiers }
    }

    /// Create a keybinding with ctrl modifier
    pub fn ctrl(c: char) -> Self {
        Self {
            code: KeyCodeConfig::Char(c),
            modifiers: Modifiers::ctrl(),
        }
    }

    /// Create a keybinding for a named key
    pub fn named(key: NamedKey) -> Self {
        Self {
            code: KeyCodeConfig::Named(key),
            modifiers: Modifiers::default(),
        }
    }

    /// Check if this keybinding matches a KeyEvent
    pub fn matches(&self, event: &KeyEvent) -> bool {
        match self.code {
            KeyCodeConfig::Char(c) => {
                // For character matching, handle case sensitivity
                match event.code {
                    KeyCode::Char(ec) => {
                        if self.modifiers.shift {
                            // Expecting uppercase: accept if character is uppercase
                            // (either via SHIFT modifier or already uppercase)
                            let char_matches = ec.to_ascii_lowercase() == c
                                && (event.modifiers.contains(KeyModifiers::SHIFT)
                                    || ec.is_ascii_uppercase());
                            // For shift-only bindings (like 'G'), don't require SHIFT in modifiers
                            // if the character itself is uppercase
                            let ctrl_match = !self.modifiers.ctrl
                                || event.modifiers.contains(KeyModifiers::CONTROL);
                            let alt_match =
                                !self.modifiers.alt || event.modifiers.contains(KeyModifiers::ALT);
                            char_matches && ctrl_match && alt_match
                        } else {
                            // Non-shift character: exact match required
                            ec == c && self.modifiers.matches(event.modifiers)
                        }
                    }
                    _ => false,
                }
            }
            KeyCodeConfig::Named(n) => {
                event.code == n.to_keycode() && self.modifiers.matches(event.modifiers)
            }
        }
    }

    /// Display string for help screen
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.alt {
            parts.push("Alt".to_string());
        }
        if self.modifiers.shift {
            match self.code {
                KeyCodeConfig::Char(c) => {
                    // For shifted characters, show uppercase
                    return if parts.is_empty() {
                        c.to_ascii_uppercase().to_string()
                    } else {
                        format!("{}-{}", parts.join("-"), c.to_ascii_uppercase())
                    };
                }
                KeyCodeConfig::Named(_) => {
                    parts.push("Shift".to_string());
                }
            }
        }

        let key_str = self.code.display();
        if parts.is_empty() {
            key_str
        } else {
            format!("{}-{}", parts.join("-"), key_str)
        }
    }
}

/// Key sequence (one or more keys)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct KeySequence(pub Vec<KeyBinding>);

impl KeySequence {
    /// Create a single-key sequence
    pub fn single(key: KeyBinding) -> Self {
        Self(vec![key])
    }

    /// Create a two-key sequence
    pub fn double(first: KeyBinding, second: KeyBinding) -> Self {
        Self(vec![first, second])
    }

    /// Check if this is a single-key sequence
    pub fn is_single(&self) -> bool {
        self.0.len() == 1
    }

    /// Get the first key (for prefix matching)
    pub fn first(&self) -> Option<&KeyBinding> {
        self.0.first()
    }

    /// Display string for help screen
    pub fn display(&self) -> String {
        self.0.iter().map(|k| k.display()).collect::<String>()
    }
}

// Custom deserializer for KeyBinding
impl<'de> Deserialize<'de> for KeyBinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(KeyBindingVisitor)
    }
}

struct KeyBindingVisitor;

impl<'de> Visitor<'de> for KeyBindingVisitor {
    type Value = KeyBinding;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string like \"j\" or an object like { key = \"d\", ctrl = true }")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        parse_key_string(v).map_err(de::Error::custom)
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut key: Option<String> = None;
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;

        while let Some(k) = map.next_key::<String>()? {
            match k.as_str() {
                "key" => {
                    key = Some(map.next_value()?);
                }
                "ctrl" => {
                    ctrl = map.next_value()?;
                }
                "shift" => {
                    shift = map.next_value()?;
                }
                "alt" => {
                    alt = map.next_value()?;
                }
                _ => {
                    let _: toml::Value = map.next_value()?;
                }
            }
        }

        let key_str = key.ok_or_else(|| de::Error::missing_field("key"))?;
        let base = parse_key_string(&key_str).map_err(de::Error::custom)?;

        // Merge explicit modifiers with any from the key string
        let modifiers = Modifiers {
            ctrl: ctrl || base.modifiers.ctrl,
            shift: shift || base.modifiers.shift,
            alt: alt || base.modifiers.alt,
        };

        Ok(KeyBinding {
            code: base.code,
            modifiers,
        })
    }
}

// Custom deserializer for KeySequence
impl<'de> Deserialize<'de> for KeySequence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(KeySequenceVisitor)
    }
}

struct KeySequenceVisitor;

impl<'de> Visitor<'de> for KeySequenceVisitor {
    type Value = KeySequence;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(
            "a string like \"j\", an object like { key = \"d\", ctrl = true }, or an array like [\"g\", \"g\"]",
        )
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let key = parse_key_string(v).map_err(de::Error::custom)?;
        Ok(KeySequence::single(key))
    }

    fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let key = KeyBindingVisitor.visit_map(map)?;
        Ok(KeySequence::single(key))
    }

    fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
    where
        S: SeqAccess<'de>,
    {
        let mut keys = Vec::new();
        while let Some(elem) = seq.next_element::<KeyBinding>()? {
            keys.push(elem);
        }
        if keys.is_empty() {
            return Err(de::Error::custom("key sequence cannot be empty"));
        }
        if keys.len() > 2 {
            return Err(de::Error::custom(
                "key sequences longer than 2 keys are not supported",
            ));
        }
        Ok(KeySequence(keys))
    }
}

/// Parse a key string like "j", "G", "Enter", or "Ctrl-d"
fn parse_key_string(s: &str) -> Result<KeyBinding, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("key string cannot be empty".to_string());
    }

    // Check for modifier prefixes
    let (modifiers, key_part) = if let Some(rest) = s
        .strip_prefix("Ctrl-")
        .or(s.strip_prefix("ctrl-"))
        .or(s.strip_prefix("C-"))
    {
        (Modifiers::ctrl(), rest)
    } else if let Some(rest) = s
        .strip_prefix("Alt-")
        .or(s.strip_prefix("alt-"))
        .or(s.strip_prefix("A-"))
    {
        (
            Modifiers {
                alt: true,
                ..Default::default()
            },
            rest,
        )
    } else if let Some(rest) = s
        .strip_prefix("Shift-")
        .or(s.strip_prefix("shift-"))
        .or(s.strip_prefix("S-"))
    {
        (Modifiers::shift(), rest)
    } else {
        (Modifiers::default(), s)
    };

    // Parse the key part
    let code = if key_part.len() == 1 {
        let c = key_part.chars().next().unwrap();
        if c.is_ascii_uppercase() && modifiers.is_empty() {
            // Uppercase letter implies shift
            return Ok(KeyBinding {
                code: KeyCodeConfig::Char(c.to_ascii_lowercase()),
                modifiers: Modifiers::shift(),
            });
        }
        KeyCodeConfig::Char(c)
    } else if let Some(named) = NamedKey::parse(key_part) {
        KeyCodeConfig::Named(named)
    } else {
        return Err(format!("unknown key: {}", key_part));
    };

    Ok(KeyBinding { code, modifiers })
}

/// Result of sequence matching
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceMatch {
    /// No match
    None,
    /// Partial match (waiting for more keys)
    Partial,
    /// Full match
    Full,
}

/// State for tracking pending key sequences
#[derive(Debug, Clone)]
pub struct SequenceState {
    pub pending_keys: smallvec::SmallVec<[KeyBinding; 4]>,
    pub pending_since: Option<Instant>,
}

impl Default for SequenceState {
    fn default() -> Self {
        Self::new()
    }
}

impl SequenceState {
    pub fn new() -> Self {
        Self {
            pending_keys: smallvec::SmallVec::new(),
            pending_since: None,
        }
    }

    /// Clear pending keys (on timeout or after match)
    pub fn clear(&mut self) {
        self.pending_keys.clear();
        self.pending_since = None;
    }

    /// Check for timeout and clear if expired
    pub fn check_timeout(&mut self) {
        if let Some(since) = self.pending_since {
            if since.elapsed() > SEQUENCE_TIMEOUT {
                self.clear();
            }
        }
    }

    /// Add a key to the pending sequence
    pub fn push(&mut self, key: KeyBinding) {
        if self.pending_keys.is_empty() {
            self.pending_since = Some(Instant::now());
        }
        self.pending_keys.push(key);
    }

    /// Try to match a sequence
    pub fn matches(&self, sequence: &KeySequence) -> SequenceMatch {
        if self.pending_keys.is_empty() {
            return SequenceMatch::None;
        }

        let pending_len = self.pending_keys.len();
        let seq_len = sequence.0.len();

        if pending_len > seq_len {
            return SequenceMatch::None;
        }

        // Check if pending keys match the prefix of the sequence
        for (i, pending) in self.pending_keys.iter().enumerate() {
            if *pending != sequence.0[i] {
                return SequenceMatch::None;
            }
        }

        if pending_len == seq_len {
            SequenceMatch::Full
        } else {
            SequenceMatch::Partial
        }
    }
}

/// Convert a KeyEvent to a KeyBinding for comparison
pub fn event_to_keybinding(event: &KeyEvent) -> Option<KeyBinding> {
    let code = match event.code {
        KeyCode::Char(c) => {
            // Normalize to lowercase for comparison
            if c.is_ascii_uppercase() {
                KeyCodeConfig::Char(c.to_ascii_lowercase())
            } else {
                KeyCodeConfig::Char(c)
            }
        }
        KeyCode::Enter => KeyCodeConfig::Named(NamedKey::Enter),
        KeyCode::Tab => KeyCodeConfig::Named(NamedKey::Tab),
        KeyCode::BackTab => KeyCodeConfig::Named(NamedKey::BackTab),
        KeyCode::Esc => KeyCodeConfig::Named(NamedKey::Esc),
        KeyCode::Backspace => KeyCodeConfig::Named(NamedKey::Backspace),
        KeyCode::Delete => KeyCodeConfig::Named(NamedKey::Delete),
        KeyCode::Up => KeyCodeConfig::Named(NamedKey::Up),
        KeyCode::Down => KeyCodeConfig::Named(NamedKey::Down),
        KeyCode::Left => KeyCodeConfig::Named(NamedKey::Left),
        KeyCode::Right => KeyCodeConfig::Named(NamedKey::Right),
        KeyCode::Home => KeyCodeConfig::Named(NamedKey::Home),
        KeyCode::End => KeyCodeConfig::Named(NamedKey::End),
        KeyCode::PageUp => KeyCodeConfig::Named(NamedKey::PageUp),
        KeyCode::PageDown => KeyCodeConfig::Named(NamedKey::PageDown),
        _ => return None,
    };

    let modifiers = Modifiers {
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        shift: event.modifiers.contains(KeyModifiers::SHIFT)
            || matches!(event.code, KeyCode::Char(c) if c.is_ascii_uppercase()),
        alt: event.modifiers.contains(KeyModifiers::ALT),
    };

    Some(KeyBinding { code, modifiers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_event_with_mods(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_parse_simple_char() {
        let key = parse_key_string("j").unwrap();
        assert_eq!(key.code, KeyCodeConfig::Char('j'));
        assert!(key.modifiers.is_empty());
    }

    #[test]
    fn test_parse_uppercase_implies_shift() {
        let key = parse_key_string("G").unwrap();
        assert_eq!(key.code, KeyCodeConfig::Char('g'));
        assert!(key.modifiers.shift);
    }

    #[test]
    fn test_parse_ctrl_modifier() {
        let key = parse_key_string("Ctrl-d").unwrap();
        assert_eq!(key.code, KeyCodeConfig::Char('d'));
        assert!(key.modifiers.ctrl);
    }

    #[test]
    fn test_parse_named_key() {
        let key = parse_key_string("Enter").unwrap();
        assert_eq!(key.code, KeyCodeConfig::Named(NamedKey::Enter));
    }

    #[test]
    fn test_keybinding_matches_char() {
        let binding = KeyBinding::char('j');
        assert!(binding.matches(&key_event(KeyCode::Char('j'))));
        assert!(!binding.matches(&key_event(KeyCode::Char('k'))));
    }

    #[test]
    fn test_keybinding_matches_uppercase() {
        let binding = KeyBinding::char('G');
        assert!(binding.matches(&key_event_with_mods(
            KeyCode::Char('G'),
            KeyModifiers::SHIFT
        )));
        assert!(binding.matches(&key_event(KeyCode::Char('G'))));
        assert!(!binding.matches(&key_event(KeyCode::Char('g'))));
    }

    #[test]
    fn test_keybinding_matches_ctrl() {
        let binding = KeyBinding::ctrl('d');
        assert!(binding.matches(&key_event_with_mods(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL
        )));
        assert!(!binding.matches(&key_event(KeyCode::Char('d'))));
    }

    #[test]
    fn test_sequence_single_key() {
        let seq = KeySequence::single(KeyBinding::char('j'));
        assert!(seq.is_single());
        assert_eq!(seq.display(), "j");
    }

    #[test]
    fn test_sequence_double_key() {
        let seq = KeySequence::double(KeyBinding::char('g'), KeyBinding::char('g'));
        assert!(!seq.is_single());
        assert_eq!(seq.display(), "gg");
    }

    #[test]
    fn test_sequence_state_matching() {
        let mut state = SequenceState::new();
        let seq = KeySequence::double(KeyBinding::char('g'), KeyBinding::char('g'));

        // Initially no match
        assert_eq!(state.matches(&seq), SequenceMatch::None);

        // After first 'g', partial match
        state.push(KeyBinding::char('g'));
        assert_eq!(state.matches(&seq), SequenceMatch::Partial);

        // After second 'g', full match
        state.push(KeyBinding::char('g'));
        assert_eq!(state.matches(&seq), SequenceMatch::Full);
    }

    #[test]
    fn test_sequence_state_no_match() {
        let mut state = SequenceState::new();
        let seq = KeySequence::double(KeyBinding::char('g'), KeyBinding::char('g'));

        state.push(KeyBinding::char('g'));
        state.push(KeyBinding::char('d')); // Different second key

        assert_eq!(state.matches(&seq), SequenceMatch::None);
    }

    #[test]
    fn test_toml_deserialize_string() {
        let toml_str = r#"key = "j""#;
        #[derive(Deserialize)]
        struct Test {
            key: KeyBinding,
        }
        let test: Test = toml::from_str(toml_str).unwrap();
        assert_eq!(test.key.code, KeyCodeConfig::Char('j'));
    }

    #[test]
    fn test_toml_deserialize_object() {
        let toml_str = r#"
            [key]
            key = "d"
            ctrl = true
        "#;
        #[derive(Deserialize)]
        struct Test {
            key: KeyBinding,
        }
        let test: Test = toml::from_str(toml_str).unwrap();
        assert_eq!(test.key.code, KeyCodeConfig::Char('d'));
        assert!(test.key.modifiers.ctrl);
    }

    #[test]
    fn test_toml_deserialize_sequence_string() {
        let toml_str = r#"key = "j""#;
        #[derive(Deserialize)]
        struct Test {
            key: KeySequence,
        }
        let test: Test = toml::from_str(toml_str).unwrap();
        assert!(test.key.is_single());
    }

    #[test]
    fn test_toml_deserialize_sequence_array() {
        let toml_str = r#"key = ["g", "g"]"#;
        #[derive(Deserialize)]
        struct Test {
            key: KeySequence,
        }
        let test: Test = toml::from_str(toml_str).unwrap();
        assert_eq!(test.key.0.len(), 2);
    }

    #[test]
    fn test_display_simple() {
        assert_eq!(KeyBinding::char('j').display(), "j");
    }

    #[test]
    fn test_display_uppercase() {
        assert_eq!(KeyBinding::char('G').display(), "G");
    }

    #[test]
    fn test_display_ctrl() {
        assert_eq!(KeyBinding::ctrl('d').display(), "Ctrl-d");
    }

    #[test]
    fn test_display_named() {
        assert_eq!(KeyBinding::named(NamedKey::Enter).display(), "Enter");
    }
}
