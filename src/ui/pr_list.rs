use ratatui::{
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    Frame,
};

use crate::app::App;
use crate::github::PullRequestSummary;

pub fn render(frame: &mut Frame, app: &mut App) {
    let has_filter_bar = app.pr_list_filter.as_ref().is_some_and(|f| f.input_active);

    let mut constraints = vec![
        Constraint::Length(3), // Header
        Constraint::Min(0),    // PR list
    ];
    if has_filter_bar {
        constraints.push(Constraint::Length(3)); // Filter bar
    }
    constraints.push(Constraint::Length(3)); // Footer

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
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
            // フィルタ適用中はフィルタ済みサブセットを表示
            let (display_prs, display_selected, total_display) =
                if let Some(ref filter) = app.pr_list_filter {
                    if filter.matched_indices.is_empty() {
                        // マッチ0件
                        let empty_msg = format!("No matches for '{}'", filter.query);
                        let empty = Paragraph::new(empty_msg)
                            .style(Style::default().fg(Color::DarkGray))
                            .block(
                                Block::default()
                                    .borders(Borders::ALL)
                                    .title(format!("Pull Requests (0/{})", prs.len())),
                            );
                        frame.render_widget(empty, chunks[1]);

                        // Filter bar
                        if has_filter_bar {
                            render_filter_bar(frame, chunks[2], filter);
                        }

                        // Footer
                        let footer_idx = if has_filter_bar { 3 } else { 2 };
                        render_footer(frame, chunks[footer_idx], app);
                        return;
                    }
                    let filtered: Vec<&PullRequestSummary> =
                        filter.matched_indices.iter().map(|&i| &prs[i]).collect();
                    let sel = filter.selected.unwrap_or(0);
                    let total = filtered.len();
                    (filtered, sel, total)
                } else {
                    let all: Vec<&PullRequestSummary> = prs.iter().collect();
                    let sel = app.selected_pr;
                    let total = all.len();
                    (all, sel, total)
                };

            let total_prs = prs.len();
            let title = if let Some(ref filter) = app.pr_list_filter {
                format!(
                    "Pull Requests ({}/{})",
                    filter.matched_indices.len(),
                    total_prs
                )
            } else if app.pr_list_loading {
                format!("Pull Requests ({}) {}", total_prs, app.spinner_char())
            } else if app.pr_list_has_more {
                format!("Pull Requests ({}+)", total_prs)
            } else {
                format!("Pull Requests ({})", total_prs)
            };

            let items = build_pr_list_items_ref(&display_prs, display_selected);

            // Use ListState for stateful rendering with automatic scroll management
            let mut list_state = ListState::default()
                .with_offset(app.pr_list_scroll_offset)
                .with_selected(Some(display_selected));

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title))
                .highlight_style(Style::default().bg(Color::DarkGray));
            frame.render_stateful_widget(list, chunks[1], &mut list_state);

            // Update scroll offset from ListState for next frame
            app.pr_list_scroll_offset = list_state.offset();

            // Scrollbar
            if total_display > 1 {
                let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(Some("▲"))
                    .end_symbol(Some("▼"));

                let mut scrollbar_state =
                    ScrollbarState::new(total_display.saturating_sub(1)).position(display_selected);

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

    // Filter bar
    if has_filter_bar {
        if let Some(ref filter) = app.pr_list_filter {
            render_filter_bar(frame, chunks[2], filter);
        }
    }

    // Footer
    let footer_idx = if has_filter_bar { 3 } else { 2 };
    render_footer(frame, chunks[footer_idx], app);
}

fn render_filter_bar(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    filter: &crate::filter::ListFilter,
) {
    let cursor_display = format!("/{}", filter.query);
    let filter_bar = Paragraph::new(Line::from(vec![
        Span::styled("Filter: ", Style::default().fg(Color::Cyan)),
        Span::styled(cursor_display, Style::default().fg(Color::White)),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(filter_bar, area);
}

fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let filter_hint = if app.pr_list_filter.is_some() {
        "Esc: clear filter | "
    } else {
        "Space /: filter | "
    };
    let footer_text = format!(
        "j/k/↑↓: move | Enter: select | {}gg/G: top/bottom | O: browser | o: open | c: closed | a: all | r: refresh | q: quit | ?: help",
        filter_hint
    );
    let footer = Paragraph::new(footer_text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, area);
}

fn build_pr_list_items_ref(prs: &[&PullRequestSummary], selected: usize) -> Vec<ListItem<'static>> {
    prs.iter()
        .enumerate()
        .map(|(i, pr)| {
            let is_selected = i == selected;

            // Draft marker
            let draft_marker = if pr.is_draft { "[DRAFT] " } else { "" };

            // PR number - yellow and bold when selected
            let number_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            };
            let number_span = Span::styled(format!("#{:<5}", pr.number), number_style);

            // Draft + Title (truncate if too long, respecting char boundaries)
            let title_width = 50;
            let full_title = format!("{}{}", draft_marker, pr.title);
            let title = truncate_string(&full_title, title_width);
            let title_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if pr.is_draft {
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
                number_span,
                Span::raw("  "),
                title_span,
                Span::raw("  "),
                author_span,
                labels_span,
            ]);

            ListItem::new(line)
        })
        .collect()
}

/// Truncate a string to fit within a given width, respecting char boundaries
fn truncate_string(s: &str, max_width: usize) -> String {
    if s.chars().count() <= max_width {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_width.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
