use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Instant;

use crate::keybinding::{event_to_keybinding, KeyBinding, KeySequence, SequenceMatch, SEQUENCE_TIMEOUT};

use super::App;

impl App {
    pub(crate) fn check_sequence_timeout(&mut self) {
        if let Some(since) = self.pending_since {
            if since.elapsed() > SEQUENCE_TIMEOUT {
                self.pending_keys.clear();
                self.pending_since = None;
            }
        }
    }

    /// Add a key to pending sequence
    pub(crate) fn push_pending_key(&mut self, key: KeyBinding) {
        if self.pending_keys.is_empty() {
            self.pending_since = Some(Instant::now());
        }
        self.pending_keys.push(key);
    }

    /// Clear pending keys
    pub(crate) fn clear_pending_keys(&mut self) {
        self.pending_keys.clear();
        self.pending_since = None;
    }

    /// Check if a KeyEvent matches a KeySequence (single-key sequences only)
    pub(crate) fn matches_single_key(&self, event: &KeyEvent, seq: &KeySequence) -> bool {
        if !seq.is_single() {
            return false;
        }
        if let Some(first) = seq.first() {
            first.matches(event)
        } else {
            false
        }
    }

    /// True for uppercase shortcuts like `J`/`K` without Ctrl/Alt modifiers.
    pub(crate) fn is_shift_char_shortcut(event: &KeyEvent, lower: char) -> bool {
        if event.modifiers.contains(KeyModifiers::CONTROL)
            || event.modifiers.contains(KeyModifiers::ALT)
        {
            return false;
        }

        let upper = lower.to_ascii_uppercase();
        match event.code {
            KeyCode::Char(c) if c == upper => true,
            KeyCode::Char(c) if c == lower && event.modifiers.contains(KeyModifiers::SHIFT) => true,
            _ => false,
        }
    }

    /// Try to match pending keys against a sequence.
    /// Returns SequenceMatch::Full if fully matched, Partial if prefix matches, None otherwise.
    pub(crate) fn try_match_sequence(&self, seq: &KeySequence) -> SequenceMatch {
        if self.pending_keys.is_empty() {
            return SequenceMatch::None;
        }

        let pending_len = self.pending_keys.len();
        let seq_len = seq.0.len();

        if pending_len > seq_len {
            return SequenceMatch::None;
        }

        // Check if pending keys match the prefix of the sequence
        for (i, pending) in self.pending_keys.iter().enumerate() {
            if *pending != seq.0[i] {
                return SequenceMatch::None;
            }
        }

        if pending_len == seq_len {
            SequenceMatch::Full
        } else {
            SequenceMatch::Partial
        }
    }

    /// Check if current key event starts or continues a sequence that could match the given sequence
    pub(crate) fn key_could_match_sequence(&self, event: &KeyEvent, seq: &KeySequence) -> bool {
        let Some(kb) = event_to_keybinding(event) else {
            return false;
        };

        // If no pending keys, check if this key matches the first key of sequence
        if self.pending_keys.is_empty() {
            if let Some(first) = seq.first() {
                return *first == kb;
            }
            return false;
        }

        // If we have pending keys, check if adding this key could complete or continue the sequence
        let pending_len = self.pending_keys.len();
        if pending_len >= seq.0.len() {
            return false;
        }

        // Check if pending keys match prefix and new key matches next position
        for (i, pending) in self.pending_keys.iter().enumerate() {
            if *pending != seq.0[i] {
                return false;
            }
        }

        seq.0
            .get(pending_len)
            .map(|expected| *expected == kb)
            .unwrap_or(false)
    }
}
