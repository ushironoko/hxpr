use std::cell::Cell;

use crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthChar;

/// テキストエリアのキー入力結果
pub enum TextAreaAction {
    /// 通常の編集操作（継続）
    Continue,
    /// Ctrl+S: 送信
    Submit,
    /// Esc: キャンセル
    Cancel,
}

/// TUI内で動作するマルチラインテキスト入力ウィジェット
pub struct TextArea {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_offset: usize,
    /// 最後にレンダリングされた領域の可視行数（ボーダー除く）
    /// render()で実際のレンダリング領域から更新される（interior mutability）
    visible_height: Cell<usize>,
}

impl Default for TextArea {
    fn default() -> Self {
        Self::new()
    }
}

impl TextArea {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            visible_height: Cell::new(1),
        }
    }

    /// キー入力を処理し、アクションを返す
    pub fn input(&mut self, key: event::KeyEvent) -> TextAreaAction {
        match key.code {
            // Ctrl+Enter で送信
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return TextAreaAction::Submit;
            }
            // Ctrl+S は後方互換性のため残す
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return TextAreaAction::Submit;
            }
            KeyCode::Esc => {
                return TextAreaAction::Cancel;
            }
            KeyCode::Char(c) => {
                self.insert_char(c);
            }
            // 通常の Enter は改行
            KeyCode::Enter => {
                self.insert_newline();
            }
            KeyCode::Backspace => {
                self.backspace();
            }
            KeyCode::Delete => {
                self.delete();
            }
            KeyCode::Left => {
                self.move_left();
            }
            KeyCode::Right => {
                self.move_right();
            }
            KeyCode::Up => {
                self.move_up();
            }
            KeyCode::Down => {
                self.move_down();
            }
            KeyCode::Home => {
                self.cursor_col = 0;
            }
            KeyCode::End => {
                self.cursor_col = self.current_line_len();
            }
            _ => {}
        }
        self.adjust_scroll();
        TextAreaAction::Continue
    }

    /// テキスト全体を返す
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    /// テキストが空かどうか
    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    /// 初期コンテンツを設定する（カーソル・スクロールをリセット）
    pub fn set_content(&mut self, content: &str) {
        self.lines = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scroll_offset = 0;
    }

    /// テキストエリアをクリアする
    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scroll_offset = 0;
    }

    /// テキストエリアをレンダリング（デフォルトタイトル・プレースホルダー）
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        self.render_with_title(
            frame,
            area,
            "Reply (Ctrl+Enter: submit, Esc: cancel)",
            "Type your reply here...",
        );
    }

    /// カスタムタイトルとプレースホルダーでレンダリング
    pub fn render_with_title(&self, frame: &mut Frame, area: Rect, title: &str, placeholder: &str) {
        let visible_height = area.height.saturating_sub(2).max(1) as usize; // borders
        self.visible_height.set(visible_height);

        let text: Vec<Line> = self
            .lines
            .iter()
            .skip(self.scroll_offset)
            .take(visible_height)
            .map(|l| Line::from(l.as_str()))
            .collect();

        let placeholder_style = Style::default().fg(Color::DarkGray);
        let display_text = if self.is_empty() {
            vec![Line::styled(placeholder, placeholder_style)]
        } else {
            text
        };

        let paragraph =
            Paragraph::new(display_text).block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(paragraph, area);

        // カーソル表示（CJK文字の表示幅を考慮）
        let cursor_x = area.x + 1 + self.cursor_display_width() as u16;
        let cursor_y = area.y + 1 + (self.cursor_row.saturating_sub(self.scroll_offset)) as u16;
        if cursor_y < area.y + area.height.saturating_sub(1) {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    fn insert_char(&mut self, c: char) {
        let line = &mut self.lines[self.cursor_row];
        // byte index from char position
        let byte_idx = char_to_byte_index(line, self.cursor_col);
        line.insert(byte_idx, c);
        self.cursor_col += 1;
    }

    fn insert_newline(&mut self) {
        let line = &self.lines[self.cursor_row];
        let byte_idx = char_to_byte_index(line, self.cursor_col);
        let rest = line[byte_idx..].to_string();
        self.lines[self.cursor_row] = line[..byte_idx].to_string();
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_row, rest);
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let byte_idx = char_to_byte_index(line, self.cursor_col);
            let prev_byte_idx = char_to_byte_index(line, self.cursor_col - 1);
            line.drain(prev_byte_idx..byte_idx);
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            // 行頭でBackspace: 前の行と結合
            let current_line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = char_count(&self.lines[self.cursor_row]);
            self.lines[self.cursor_row].push_str(&current_line);
        }
    }

    fn delete(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_col < line_len {
            let line = &mut self.lines[self.cursor_row];
            let byte_idx = char_to_byte_index(line, self.cursor_col);
            let next_byte_idx = char_to_byte_index(line, self.cursor_col + 1);
            line.drain(byte_idx..next_byte_idx);
        } else if self.cursor_row + 1 < self.lines.len() {
            // 行末でDelete: 次の行と結合
            let next_line = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next_line);
        }
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
        }
    }

    fn move_right(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.cursor_col.min(self.current_line_len());
        }
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = self.cursor_col.min(self.current_line_len());
        }
    }

    fn current_line_len(&self) -> usize {
        char_count(&self.lines[self.cursor_row])
    }

    /// カーソル位置までの表示幅を計算する（CJK文字は2カラム幅）
    fn cursor_display_width(&self) -> usize {
        let line = &self.lines[self.cursor_row];
        line.chars()
            .take(self.cursor_col)
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum()
    }

    fn adjust_scroll(&mut self) {
        // スクロール調整: カーソルが見えるように（render()で設定された実際の可視高さを使用）
        let visible_height = self.visible_height.get();
        if self.cursor_row < self.scroll_offset {
            self.scroll_offset = self.cursor_row;
        }
        if self.cursor_row >= self.scroll_offset + visible_height {
            self.scroll_offset = self.cursor_row.saturating_sub(visible_height) + 1;
        }
    }
}

/// 文字数を数える（マルチバイト対応）
fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// 文字インデックスからバイトインデックスへ変換
fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctrl_key_event(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_new_text_area_is_empty() {
        let ta = TextArea::new();
        assert!(ta.is_empty());
        assert_eq!(ta.content(), "");
    }

    #[test]
    fn test_insert_char() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('h')));
        ta.input(key_event(KeyCode::Char('i')));
        assert_eq!(ta.content(), "hi");
        assert!(!ta.is_empty());
    }

    #[test]
    fn test_insert_newline() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Enter));
        ta.input(key_event(KeyCode::Char('b')));
        assert_eq!(ta.content(), "a\nb");
    }

    #[test]
    fn test_backspace() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Char('b')));
        ta.input(key_event(KeyCode::Backspace));
        assert_eq!(ta.content(), "a");
    }

    #[test]
    fn test_backspace_at_line_start_joins_lines() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Enter));
        ta.input(key_event(KeyCode::Char('b')));
        ta.input(key_event(KeyCode::Home));
        ta.input(key_event(KeyCode::Backspace));
        assert_eq!(ta.content(), "ab");
    }

    #[test]
    fn test_delete_joins_lines() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Enter));
        ta.input(key_event(KeyCode::Char('b')));
        // Move to end of first line
        ta.input(key_event(KeyCode::Up));
        ta.input(key_event(KeyCode::End));
        ta.input(key_event(KeyCode::Delete));
        assert_eq!(ta.content(), "ab");
    }

    #[test]
    fn test_cursor_movement() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Char('b')));
        ta.input(key_event(KeyCode::Char('c')));
        ta.input(key_event(KeyCode::Left));
        ta.input(key_event(KeyCode::Left));
        ta.input(key_event(KeyCode::Char('x')));
        assert_eq!(ta.content(), "axbc");
    }

    #[test]
    fn test_submit_action() {
        let mut ta = TextArea::new();
        let action = ta.input(ctrl_key_event(KeyCode::Char('s')));
        assert!(matches!(action, TextAreaAction::Submit));
    }

    #[test]
    fn test_ctrl_enter_submit() {
        let mut ta = TextArea::new();
        let action = ta.input(ctrl_key_event(KeyCode::Enter));
        assert!(matches!(action, TextAreaAction::Submit));
    }

    #[test]
    fn test_enter_inserts_newline_not_submit() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('a')));
        let action = ta.input(key_event(KeyCode::Enter)); // Ctrl なし
        assert!(matches!(action, TextAreaAction::Continue));
        ta.input(key_event(KeyCode::Char('b')));
        assert_eq!(ta.content(), "a\nb");
    }

    #[test]
    fn test_cancel_action() {
        let mut ta = TextArea::new();
        let action = ta.input(key_event(KeyCode::Esc));
        assert!(matches!(action, TextAreaAction::Cancel));
    }

    #[test]
    fn test_set_content() {
        let mut ta = TextArea::new();
        ta.set_content("line1\nline2");
        assert_eq!(ta.content(), "line1\nline2");
        assert_eq!(ta.cursor_row, 0);
        assert_eq!(ta.cursor_col, 0);
    }

    #[test]
    fn test_set_content_empty() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('x')));
        ta.set_content("");
        assert!(ta.is_empty());
        assert_eq!(ta.cursor_row, 0);
        assert_eq!(ta.cursor_col, 0);
    }

    #[test]
    fn test_clear() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Enter));
        ta.input(key_event(KeyCode::Char('b')));
        ta.clear();
        assert!(ta.is_empty());
        assert_eq!(ta.cursor_row, 0);
        assert_eq!(ta.cursor_col, 0);
    }

    #[test]
    fn test_multibyte_chars() {
        let mut ta = TextArea::new();
        ta.input(key_event(KeyCode::Char('あ')));
        ta.input(key_event(KeyCode::Char('い')));
        assert_eq!(ta.content(), "あい");
        ta.input(key_event(KeyCode::Backspace));
        assert_eq!(ta.content(), "あ");
    }

    #[test]
    fn test_cjk_cursor_display_width() {
        let mut ta = TextArea::new();
        // "あい" を入力 → cursor_col = 2, 表示幅 = 4
        ta.input(key_event(KeyCode::Char('あ')));
        ta.input(key_event(KeyCode::Char('い')));
        assert_eq!(ta.cursor_col, 2);
        assert_eq!(ta.cursor_display_width(), 4);
    }

    #[test]
    fn test_mixed_ascii_cjk_cursor_display_width() {
        let mut ta = TextArea::new();
        // "aあb" → cursor_col = 3, 表示幅 = 1+2+1 = 4
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Char('あ')));
        ta.input(key_event(KeyCode::Char('b')));
        assert_eq!(ta.cursor_col, 3);
        assert_eq!(ta.cursor_display_width(), 4);
    }

    #[test]
    fn test_ascii_only_cursor_display_width() {
        let mut ta = TextArea::new();
        // ASCII のみ → cursor_col と表示幅が一致
        ta.input(key_event(KeyCode::Char('a')));
        ta.input(key_event(KeyCode::Char('b')));
        ta.input(key_event(KeyCode::Char('c')));
        assert_eq!(ta.cursor_col, 3);
        assert_eq!(ta.cursor_display_width(), 3);
    }
}
