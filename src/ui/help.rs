use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

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
        Line::from("  Enter           View file diff"),
        Line::from("  a               Approve PR"),
        Line::from("  r               Request changes"),
        Line::from("  m               Comment only"),
        Line::from("  ?               Toggle help"),
        Line::from("  q               Quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Diff View",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  j/k, Down/Up    Move line selection"),
        Line::from("  Ctrl-d          Page down"),
        Line::from("  Ctrl-u          Page up"),
        Line::from("  c               Add comment at line"),
        Line::from("  s               Add suggestion at line"),
        Line::from("  q, Esc          Back to file list"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Comment/Suggestion Preview",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from("  Enter           Submit comment/suggestion"),
        Line::from("  Esc             Cancel"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press q or ? to close this help",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let help = Paragraph::new(help_lines).block(Block::default().borders(Borders::ALL));
    frame.render_widget(help, chunks[1]);
}
