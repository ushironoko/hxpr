use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::syntax::available_themes;

pub fn render(frame: &mut Frame, _app: &App) {
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

    // Help content
    let help_lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "File List View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  j/k, Down/Up    Move selection"),
        Line::from("  Enter           Open split view"),
        Line::from("  a               Approve PR"),
        Line::from("  r               Request changes"),
        Line::from("  c               Comment only"),
        Line::from("  C               View review comments"),
        Line::from("  A               Start AI Rally"),
        Line::from("  R               Refresh (clear cache and reload)"),
        Line::from("  ?               Toggle help"),
        Line::from("  q               Quit"),
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
        Line::from("  j/k, Down/Up    Move file selection (diff follows)"),
        Line::from("  Enter, →, l     Focus diff pane"),
        Line::from("  ←, h, q         Back to file list"),
        Line::from(vec![Span::styled(
            "  Diff Focus:",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from("  j/k, Down/Up    Scroll diff"),
        Line::from("  Ctrl-d/u        Page scroll"),
        Line::from("  gd              Go to definition"),
        Line::from("  gf              Open file in $EDITOR"),
        Line::from("  gg/G            Jump to first/last line"),
        Line::from("  Ctrl-o          Jump back"),
        Line::from("  n/N             Next/prev comment"),
        Line::from("  Enter           Open comment panel"),
        Line::from("  →/l             Open fullscreen diff"),
        Line::from("  ←, h            Back to file focus"),
        Line::from("  q               Back to file list"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Diff View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  j/k, Down/Up    Move line selection"),
        Line::from("  gd              Go to definition"),
        Line::from("  gf              Open file in $EDITOR"),
        Line::from("  gg/G            Jump to first/last line"),
        Line::from("  Ctrl-o          Jump back"),
        Line::from("  n               Jump to next comment"),
        Line::from("  N               Jump to previous comment"),
        Line::from("  Enter           Open comment panel"),
        Line::from("  Ctrl-d          Page down"),
        Line::from("  Ctrl-u          Page up"),
        Line::from("  c               Add comment at line"),
        Line::from("  s               Add suggestion at line"),
        Line::from("  q, Esc          Back to file list"),
        Line::from(vec![Span::styled(
            "  Comment Panel (focused):",
            Style::default().fg(Color::DarkGray),
        )]),
        Line::from("  j/k             Scroll panel"),
        Line::from("  c               Add comment"),
        Line::from("  s               Add suggestion"),
        Line::from("  r               Reply to comment"),
        Line::from("  Tab/Shift-Tab   Select reply target (multiple)"),
        Line::from("  n/N             Jump to next/prev comment"),
        Line::from("  Esc/q           Close panel"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Comment List View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  [, ]            Switch tab (Review/Discussion)"),
        Line::from("  j/k, Down/Up    Move selection"),
        Line::from("  Enter           Review: Jump to file | Discussion: View detail"),
        Line::from("  q, Esc          Back to file list"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Input Mode (Comment/Suggestion/Reply)",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  Ctrl+Enter      Submit comment/suggestion/reply"),
        Line::from("  Ctrl+S          Submit (alternative)"),
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
        Line::from("  q               Abort rally"),
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
            "Press q or ? to close this help",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let help = Paragraph::new(help_lines).block(Block::default().borders(Borders::ALL));
    frame.render_widget(help, chunks[1]);
}
