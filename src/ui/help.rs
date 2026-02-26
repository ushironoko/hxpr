use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::app::App;
use crate::config::KeybindingsConfig;
use crate::syntax::available_themes;

/// Format a key display with padding for alignment
fn fmt_key(key: &str, width: usize) -> String {
    format!("  {:<width$}", key, width = width)
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Help content
        ])
        .split(frame.area());

    // Title
    let title = Paragraph::new("octorus - GitHub PR Review TUI")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL).title("Help"));
    frame.render_widget(title, chunks[0]);

    // Get keybindings from config
    let kb = &app.config.keybindings;

    // Help content with dynamic keybindings
    let help_lines = build_help_lines(kb);
    let total_lines = help_lines.len();
    // Content area height (subtract 2 for borders)
    let content_height = chunks[1].height.saturating_sub(2) as usize;

    // Clamp scroll offset so we don't scroll past content
    let max_scroll = total_lines.saturating_sub(content_height);
    if app.help_scroll_offset > max_scroll {
        app.help_scroll_offset = max_scroll;
    }

    let scroll_info = if total_lines > content_height {
        format!(
            " ({}/{})",
            app.help_scroll_offset + 1,
            max_scroll + 1
        )
    } else {
        String::new()
    };

    let help = Paragraph::new(help_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Keybindings{}", scroll_info)),
        )
        .scroll((app.help_scroll_offset as u16, 0));
    frame.render_widget(help, chunks[1]);

    // Scrollbar
    if total_lines > content_height {
        let mut scrollbar_state = ScrollbarState::new(max_scroll + 1)
            .position(app.help_scroll_offset);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        frame.render_stateful_widget(
            scrollbar,
            chunks[1].inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn build_help_lines(kb: &KeybindingsConfig) -> Vec<Line<'static>> {
    let key_width = 14; // Width for key column

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "File List View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!(
            "{}  Move selection",
            fmt_key(
                &format!(
                    "{}/{}, Down/Up",
                    kb.move_down.display(),
                    kb.move_up.display()
                ),
                key_width
            )
        )),
        Line::from(format!(
            "{}  Open split view",
            fmt_key(&kb.open_panel.display(), key_width)
        )),
        Line::from("  v               Mark selected file as viewed"),
        Line::from("  V               Mark selected directory as viewed"),
        Line::from(format!(
            "{}  Approve PR",
            fmt_key(&kb.approve.display(), key_width)
        )),
        Line::from(format!(
            "{}  Request changes",
            fmt_key(&kb.request_changes.display(), key_width)
        )),
        Line::from(format!(
            "{}  Comment only",
            fmt_key(&kb.comment.display(), key_width)
        )),
        Line::from(format!(
            "{}  View review comments",
            fmt_key(&kb.comment_list.display(), key_width)
        )),
        Line::from(format!(
            "{}  Start AI Rally",
            fmt_key(&kb.ai_rally.display(), key_width)
        )),
        Line::from(format!(
            "{}  Open PR in browser",
            fmt_key(&kb.open_in_browser.display(), key_width)
        )),
        Line::from(format!(
            "{}  Refresh (clear cache and reload)",
            fmt_key(&kb.refresh.display(), key_width)
        )),
        Line::from(format!(
            "{}  Toggle help",
            fmt_key(&kb.help.display(), key_width)
        )),
        Line::from(format!(
            "{}  Toggle local diff mode",
            fmt_key(&kb.toggle_local_mode.display(), key_width)
        )),
        Line::from(format!(
            "{}  Toggle auto-focus (local mode)",
            fmt_key(&kb.toggle_auto_focus.display(), key_width)
        )),
        Line::from(format!(
            "{}  Filter list",
            fmt_key(&kb.filter.display(), key_width)
        )),
        Line::from(format!("{}  Quit", fmt_key(&kb.quit.display(), key_width))),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Split View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "  File List Focus:",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(format!(
            "{}  Move file selection (diff follows)",
            fmt_key(
                &format!(
                    "{}/{}, Down/Up",
                    kb.move_down.display(),
                    kb.move_up.display()
                ),
                key_width
            )
        )),
        Line::from(format!(
            "{}  Filter list",
            fmt_key(&kb.filter.display(), key_width)
        )),
        Line::from(format!(
            "{}, Right, {}     Focus diff pane",
            fmt_key(&kb.open_panel.display(), 5),
            kb.move_right.display()
        )),
        Line::from(format!(
            "  Left, {}, {}    Back to file list",
            kb.move_left.display(),
            kb.quit.display()
        )),
        Line::from(vec![Span::styled(
            "  Diff Focus:",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(format!(
            "{}  Scroll diff",
            fmt_key(
                &format!(
                    "{}/{}, Down/Up",
                    kb.move_down.display(),
                    kb.move_up.display()
                ),
                key_width
            )
        )),
        Line::from(format!(
            "{}  Page scroll (also J/K)",
            fmt_key(
                &format!("{}/{}", kb.page_down.display(), kb.page_up.display()),
                key_width
            )
        )),
        Line::from(format!(
            "{}  Go to definition",
            fmt_key(&kb.go_to_definition.display(), key_width)
        )),
        Line::from(format!(
            "{}  Open file in $EDITOR",
            fmt_key(&kb.go_to_file.display(), key_width)
        )),
        Line::from(format!(
            "{}/{}  Jump to first/last line",
            fmt_key(&kb.jump_to_first.display(), 10),
            kb.jump_to_last.display()
        )),
        Line::from(format!(
            "{}  Jump back",
            fmt_key(&kb.jump_back.display(), key_width)
        )),
        Line::from(format!(
            "{}/{}  Next/prev comment",
            fmt_key(&kb.next_comment.display(), 10),
            kb.prev_comment.display()
        )),
        Line::from(format!(
            "{}  Open comment panel",
            fmt_key(&kb.open_panel.display(), key_width)
        )),
        Line::from(format!(
            "  Right, {}       Open fullscreen diff",
            kb.move_right.display()
        )),
        Line::from(format!(
            "  Left, {}        Back to file focus",
            kb.move_left.display()
        )),
        Line::from(format!(
            "{}  Back to file list",
            fmt_key(&kb.quit.display(), key_width)
        )),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Diff View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!(
            "{}  Move line selection",
            fmt_key(
                &format!(
                    "{}/{}, Down/Up",
                    kb.move_down.display(),
                    kb.move_up.display()
                ),
                key_width
            )
        )),
        Line::from(format!(
            "{}  Go to definition",
            fmt_key(&kb.go_to_definition.display(), key_width)
        )),
        Line::from(format!(
            "{}  Open file in $EDITOR",
            fmt_key(&kb.go_to_file.display(), key_width)
        )),
        Line::from(format!(
            "{}/{}  Jump to first/last line",
            fmt_key(&kb.jump_to_first.display(), 10),
            kb.jump_to_last.display()
        )),
        Line::from(format!(
            "{}  Jump back",
            fmt_key(&kb.jump_back.display(), key_width)
        )),
        Line::from(format!(
            "{}  Jump to next comment",
            fmt_key(&kb.next_comment.display(), key_width)
        )),
        Line::from(format!(
            "{}  Jump to previous comment",
            fmt_key(&kb.prev_comment.display(), key_width)
        )),
        Line::from(format!(
            "{}  Open comment panel",
            fmt_key(&kb.open_panel.display(), key_width)
        )),
        Line::from(format!(
            "{}  Page down (also J)",
            fmt_key(&kb.page_down.display(), key_width)
        )),
        Line::from(format!(
            "{}  Page up (also K)",
            fmt_key(&kb.page_up.display(), key_width)
        )),
        Line::from(format!(
            "{}  Add comment at line",
            fmt_key(&kb.comment.display(), key_width)
        )),
        Line::from(format!(
            "{}  Add suggestion at line",
            fmt_key(&kb.suggestion.display(), key_width)
        )),
        Line::from(format!(
            "{}  Multiline select mode",
            fmt_key("Shift+Enter", key_width)
        )),
        Line::from(vec![Span::styled(
            "  Multiline Select Mode:",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(format!(
            "{}/{}  Extend selection",
            fmt_key(&kb.move_down.display(), 10),
            kb.move_up.display()
        )),
        Line::from(format!(
            "{}  Comment on selection",
            fmt_key("Enter/c", key_width)
        )),
        Line::from(format!(
            "{}  Suggest on selection",
            fmt_key(&kb.suggestion.display(), key_width)
        )),
        Line::from(format!(
            "{}  Cancel selection",
            fmt_key("Esc", key_width)
        )),
        Line::from(format!(
            "{}  Toggle markdown rich display",
            fmt_key(&kb.toggle_markdown_rich.display(), key_width)
        )),
        Line::from(format!(
            "{}  Back to file list",
            fmt_key(&format!("{}, Esc", kb.quit.display()), key_width)
        )),
        Line::from(vec![Span::styled(
            "  Comment Panel (focused):",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(format!(
            "{}/{}  Scroll panel",
            fmt_key(&kb.move_down.display(), 10),
            kb.move_up.display()
        )),
        Line::from(format!(
            "{}  Add comment",
            fmt_key(&kb.comment.display(), key_width)
        )),
        Line::from(format!(
            "{}  Add suggestion",
            fmt_key(&kb.suggestion.display(), key_width)
        )),
        Line::from(format!(
            "{}  Reply to comment",
            fmt_key(&kb.reply.display(), key_width)
        )),
        Line::from("  Tab/Shift-Tab   Select reply target (multiple)"),
        Line::from(format!(
            "{}/{}  Jump to next/prev comment",
            fmt_key(&kb.next_comment.display(), 10),
            kb.prev_comment.display()
        )),
        Line::from(format!("  Esc/{}        Close panel", kb.quit.display())),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Comment List View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  [, ]            Switch tab (Review/Discussion)"),
        Line::from(format!(
            "{}  Move selection",
            fmt_key(
                &format!(
                    "{}/{}, Down/Up",
                    kb.move_down.display(),
                    kb.move_up.display()
                ),
                key_width
            )
        )),
        Line::from(format!(
            "{}  Review: Jump to file | Discussion: View detail",
            fmt_key(&kb.open_panel.display(), key_width)
        )),
        Line::from(format!(
            "{}  Back to file list",
            fmt_key(&format!("{}, Esc", kb.quit.display()), key_width)
        )),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Input Mode (Comment/Suggestion/Reply)",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!(
            "{}  Submit",
            fmt_key(&kb.submit.display(), key_width)
        )),
        Line::from("  Esc             Cancel input"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "AI Rally View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            "  (When AI requests permission or clarification)",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from("  y               Grant permission / Answer yes"),
        Line::from("  n               Deny permission / Skip"),
        Line::from(format!(
            "{}  Abort rally",
            fmt_key(&kb.quit.display(), key_width)
        )),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Available Themes",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("  {}", available_themes().join(", "))),
        Line::from(vec![Span::styled(
            "  Set in ~/.config/octorus/config.toml: [diff] theme = \"Dracula\"",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            format!(
                "Press {} or {} to close this help | j/k: scroll | J/K: page scroll | Ctrl-d/u: half page | g/G: top/bottom",
                kb.quit.display(),
                kb.help.display()
            ),
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    lines
}
