use ratatui::{
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::app::App;
use crate::github::PullRequestSummary;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // PR list
            Constraint::Length(3), // Footer
        ])
        .split(frame.area());

    // Header
    let filter_str = app.pr_list_state_filter.display_name();
    let header_text = format!("PR List: {} ({})", app.repo, filter_str);
    let header =
        Paragraph::new(header_text).block(Block::default().borders(Borders::ALL).title("octorus"));
    frame.render_widget(header, chunks[0]);

    // PR list
    if app.pr_list_loading && app.pr_list.is_none() {
        // 初回ローディング
        let loading = Paragraph::new(format!("{} Loading PRs...", app.spinner_char())).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Pull Requests"),
        );
        frame.render_widget(loading, chunks[1]);
    } else if let Some(ref prs) = app.pr_list {
        if prs.is_empty() {
            let empty = Paragraph::new("No pull requests found").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Pull Requests"),
            );
            frame.render_widget(empty, chunks[1]);
        } else {
            let total_prs = prs.len();
            let items = build_pr_list_items(prs, app.selected_pr);

            let title = if app.pr_list_loading {
                format!("Pull Requests ({}) {}", total_prs, app.spinner_char())
            } else if app.pr_list_has_more {
                format!("Pull Requests ({}+)", total_prs)
            } else {
                format!("Pull Requests ({})", total_prs)
            };

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title))
                .highlight_style(Style::default().bg(Color::DarkGray));
            frame.render_widget(list, chunks[1]);

            // Scrollbar
            if total_prs > 1 {
                let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("▲"))
                    .end_symbol(Some("▼"));

                let mut scrollbar_state =
                    ScrollbarState::new(total_prs.saturating_sub(1)).position(app.selected_pr);

                frame.render_stateful_widget(
                    scrollbar,
                    chunks[1].inner(Margin {
                        vertical: 1,
                        horizontal: 0,
                    }),
                    &mut scrollbar_state,
                );
            }
        }
    } else {
        // pr_list が None (エラー時など)
        let empty = Paragraph::new("Failed to load pull requests").block(
            Block::default()
                .borders(Borders::ALL)
                .title("Pull Requests"),
        );
        frame.render_widget(empty, chunks[1]);
    }

    // Footer
    let footer_text =
        "j/k/↑↓: move | Enter: select | gg/G: top/bottom | o: open | c: closed | a: all | r: refresh | q: quit | ?: help";
    let footer = Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, chunks[2]);
}

fn build_pr_list_items(prs: &[PullRequestSummary], selected: usize) -> Vec<ListItem<'static>> {
    prs.iter()
        .enumerate()
        .map(|(i, pr)| {
            let is_selected = i == selected;

            // Draft marker
            let draft_marker = if pr.is_draft { "[DRAFT] " } else { "" };

            // State color
            let _state_style = match pr.state.as_str() {
                "OPEN" => Style::default().fg(Color::Green),
                "CLOSED" => Style::default().fg(Color::Red),
                "MERGED" => Style::default().fg(Color::Magenta),
                _ => Style::default(),
            };

            // PR number
            let number_span = Span::styled(
                format!("#{:<5}", pr.number),
                Style::default().fg(Color::Yellow),
            );

            // Draft + Title (truncate if too long)
            let title_width = 50;
            let full_title = format!("{}{}", draft_marker, pr.title);
            let title = if full_title.len() > title_width {
                format!("{}...", &full_title[..title_width - 3])
            } else {
                full_title
            };
            let title_style = if pr.is_draft {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let title_span = Span::styled(
                format!("{:<width$}", title, width = title_width),
                title_style,
            );

            // Author
            let author_span = Span::styled(
                format!("by @{}", pr.author.login),
                Style::default().fg(Color::Cyan),
            );

            // Labels (show first 2)
            let labels_str = if !pr.labels.is_empty() {
                let label_names: Vec<&str> =
                    pr.labels.iter().take(2).map(|l| l.name.as_str()).collect();
                if pr.labels.len() > 2 {
                    format!(" [{}+{}]", label_names.join(", "), pr.labels.len() - 2)
                } else {
                    format!(" [{}]", label_names.join(", "))
                }
            } else {
                String::new()
            };
            let labels_span = Span::styled(labels_str, Style::default().fg(Color::Blue));

            let line = Line::from(vec![
                Span::raw(if is_selected { "▶ " } else { "  " }),
                number_span,
                Span::raw("  "),
                title_span,
                Span::raw("  "),
                author_span,
                labels_span,
            ]);

            let style = if is_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect()
}
