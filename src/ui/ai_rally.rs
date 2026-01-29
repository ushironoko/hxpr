use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

use crate::ai::{RallyState, ReviewAction, RevieweeStatus};
use crate::app::{AiRallyState, App, LogEntry, LogEventType};

pub fn render(frame: &mut Frame, app: &mut App) {
    let Some(rally_state) = &mut app.ai_rally_state else {
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(3), // Status bar
        ])
        .split(frame.area());

    render_header(frame, chunks[0], rally_state);
    render_main_content(frame, chunks[1], rally_state);
    render_status_bar(frame, chunks[2], rally_state);

    // Render modal on top if showing log detail
    if rally_state.showing_log_detail {
        render_log_detail_modal(frame, rally_state);
    }
}

fn render_header(frame: &mut Frame, area: Rect, state: &AiRallyState) {
    let state_text = match state.state {
        RallyState::Initializing => "Initializing...",
        RallyState::ReviewerReviewing => "Reviewer reviewing...",
        RallyState::RevieweeFix => "Reviewee fixing...",
        RallyState::WaitingForClarification => "Waiting for clarification",
        RallyState::WaitingForPermission => "Waiting for permission",
        RallyState::Completed => "Completed!",
        RallyState::Aborted => "Aborted",
        RallyState::Error => "Error",
    };

    let state_color = match state.state {
        RallyState::Initializing => Color::Blue,
        RallyState::ReviewerReviewing => Color::Yellow,
        RallyState::RevieweeFix => Color::Cyan,
        RallyState::WaitingForClarification | RallyState::WaitingForPermission => Color::Magenta,
        RallyState::Completed => Color::Green,
        RallyState::Aborted => Color::Yellow,
        RallyState::Error => Color::Red,
    };

    let title = format!(
        " AI Rally - Iteration {}/{} ",
        state.iteration, state.max_iterations
    );

    let header = Paragraph::new(Line::from(vec![
        Span::styled("Status: ", Style::default().fg(Color::Gray)),
        Span::styled(
            state_text,
            Style::default()
                .fg(state_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(state_color)),
    );

    frame.render_widget(header, area);
}

fn render_main_content(frame: &mut Frame, area: Rect, state: &mut AiRallyState) {
    // Add waiting prompt area when in clarification/permission state
    let is_waiting = matches!(
        state.state,
        RallyState::WaitingForClarification | RallyState::WaitingForPermission
    );

    let chunks = if is_waiting {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(35), // History
                Constraint::Length(6),      // Waiting prompt
                Constraint::Min(10),        // Logs
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50), // History
                Constraint::Percentage(50), // Logs
            ])
            .split(area)
    };

    render_history(frame, chunks[0], state);

    if is_waiting {
        render_waiting_prompt(frame, chunks[1], state);
        render_logs(frame, chunks[2], state);
    } else {
        render_logs(frame, chunks[1], state);
    }
}

fn render_waiting_prompt(frame: &mut Frame, area: Rect, state: &AiRallyState) {
    let (title, content, help) = match state.state {
        RallyState::WaitingForClarification => {
            let question = state
                .pending_question
                .as_deref()
                .unwrap_or("(No question provided)");
            (
                " Clarification Required ",
                format!("Question: {}", question),
                "Press 'y' to open editor and respond, 'n' to skip, 'q' to abort",
            )
        }
        RallyState::WaitingForPermission => {
            let (action, reason) = state
                .pending_permission
                .as_ref()
                .map(|p| (p.action.as_str(), p.reason.as_str()))
                .unwrap_or(("(No action)", "(No reason)"));
            (
                " Permission Required ",
                format!("Action: {}\nReason: {}", action, reason),
                "Press 'y' to approve, 'n' to deny, 'q' to abort",
            )
        }
        _ => return,
    };

    let lines = vec![
        Line::from(vec![Span::styled(
            content,
            Style::default().fg(Color::White),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(help, Style::default().fg(Color::Yellow))]),
    ];

    let prompt = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Magenta)),
    );

    frame.render_widget(prompt, area);
}

fn render_history(frame: &mut Frame, area: Rect, state: &AiRallyState) {
    let visible_height = area.height.saturating_sub(2) as usize;

    let items: Vec<ListItem> = state
        .history
        .iter()
        .filter_map(|event| {
            let (prefix, content, color) = match event {
                crate::ai::orchestrator::RallyEvent::IterationStarted(i) => (
                    format!("[{}]", i),
                    "Iteration started".to_string(),
                    Color::Blue,
                ),
                crate::ai::orchestrator::RallyEvent::ReviewCompleted(review) => {
                    let action_text = match review.action {
                        ReviewAction::Approve => "APPROVE",
                        ReviewAction::RequestChanges => "REQUEST_CHANGES",
                        ReviewAction::Comment => "COMMENT",
                    };
                    let color = match review.action {
                        ReviewAction::Approve => Color::Green,
                        ReviewAction::RequestChanges => Color::Red,
                        ReviewAction::Comment => Color::Yellow,
                    };
                    (
                        format!("Review: {}", action_text),
                        truncate_string(&review.summary, 60),
                        color,
                    )
                }
                crate::ai::orchestrator::RallyEvent::FixCompleted(fix) => {
                    let status_text = match fix.status {
                        RevieweeStatus::Completed => "COMPLETED",
                        RevieweeStatus::NeedsClarification => "NEEDS_CLARIFICATION",
                        RevieweeStatus::NeedsPermission => "NEEDS_PERMISSION",
                        RevieweeStatus::Error => "ERROR",
                    };
                    let color = match fix.status {
                        RevieweeStatus::Completed => Color::Green,
                        RevieweeStatus::NeedsClarification | RevieweeStatus::NeedsPermission => {
                            Color::Yellow
                        }
                        RevieweeStatus::Error => Color::Red,
                    };
                    (
                        format!("Fix: {}", status_text),
                        truncate_string(&fix.summary, 60),
                        color,
                    )
                }
                crate::ai::orchestrator::RallyEvent::ClarificationNeeded(q) => (
                    "Clarification".to_string(),
                    truncate_string(q, 60),
                    Color::Magenta,
                ),
                crate::ai::orchestrator::RallyEvent::PermissionNeeded(action, _) => (
                    "Permission".to_string(),
                    truncate_string(action, 60),
                    Color::Magenta,
                ),
                crate::ai::orchestrator::RallyEvent::Approved(summary) => (
                    "APPROVED".to_string(),
                    truncate_string(summary, 60),
                    Color::Green,
                ),
                crate::ai::orchestrator::RallyEvent::Error(e) => {
                    ("ERROR".to_string(), truncate_string(e, 60), Color::Red)
                }
                _ => return None,
            };

            Some(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{}: ", prefix),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(content, Style::default().fg(Color::White)),
            ])))
        })
        .collect();

    // Auto-scroll to show latest history entries
    let total = items.len();
    let scroll_offset = total.saturating_sub(visible_height);
    let visible_items: Vec<ListItem> = items.into_iter().skip(scroll_offset).collect();

    let list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" History ")
            .border_style(Style::default().fg(Color::Gray)),
    );

    frame.render_widget(list, area);
}

fn render_logs(frame: &mut Frame, area: Rect, state: &mut AiRallyState) {
    let visible_height = area.height.saturating_sub(2) as usize; // subtract borders
    state.last_visible_log_height = visible_height;
    let total_logs = state.logs.len();

    // Calculate scroll position (auto-scroll to bottom by default unless user has scrolled up)
    let scroll_offset = if state.log_scroll_offset == 0 {
        // Auto-scroll: show latest logs
        total_logs.saturating_sub(visible_height)
    } else {
        state.log_scroll_offset
    };

    let items: Vec<ListItem> = state
        .logs
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(idx, entry)| {
            let is_selected = state.selected_log_index == Some(idx);
            format_log_entry(entry, is_selected)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Logs ({}/{}) [j/k/↑↓: select, Enter: detail] ",
            scroll_offset.saturating_add(visible_height).min(total_logs),
            total_logs
        ))
        .border_style(Style::default().fg(Color::Gray));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let list = List::new(items);
    frame.render_widget(list, inner_area);

    // Render scrollbar if there are more logs than visible
    if total_logs > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

        let mut scrollbar_state =
            ScrollbarState::new(total_logs.saturating_sub(visible_height)).position(scroll_offset);

        frame.render_stateful_widget(
            scrollbar,
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn format_log_entry(entry: &LogEntry, is_selected: bool) -> ListItem<'static> {
    // Use ASCII characters for better terminal compatibility
    // Some terminals may not render emojis correctly
    let (icon, color) = match entry.event_type {
        LogEventType::Info => ("[i]", Color::Blue),
        LogEventType::Thinking => ("[~]", Color::Magenta),
        LogEventType::ToolUse => ("[>]", Color::Cyan),
        LogEventType::ToolResult => ("[+]", Color::Green),
        LogEventType::Text => ("[.]", Color::White),
        LogEventType::Review => ("[R]", Color::Yellow),
        LogEventType::Fix => ("[F]", Color::Cyan),
        LogEventType::Error => ("[!]", Color::Red),
    };

    let type_label = match entry.event_type {
        LogEventType::Info => "Info",
        LogEventType::Thinking => "Think",
        LogEventType::ToolUse => "Tool",
        LogEventType::ToolResult => "Result",
        LogEventType::Text => "Output",
        LogEventType::Review => "Review",
        LogEventType::Fix => "Fix",
        LogEventType::Error => "Error",
    };

    // Selection indicator
    let selector = if is_selected { ">" } else { " " };

    // Truncate message for list display (full content available in detail modal)
    let display_message = truncate_string(&entry.message, 80);

    let mut item = ListItem::new(Line::from(vec![
        Span::styled(
            selector.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}] ", entry.timestamp),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(format!("{} ", icon), Style::default().fg(color)),
        Span::styled(
            format!("{}: ", type_label),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(display_message, Style::default().fg(Color::White)),
    ]));

    // Highlight selected row
    if is_selected {
        item = item.style(Style::default().bg(Color::DarkGray));
    }

    item
}

fn render_log_detail_modal(frame: &mut Frame, state: &AiRallyState) {
    let Some(selected_idx) = state.selected_log_index else {
        return;
    };

    let Some(entry) = state.logs.get(selected_idx) else {
        return;
    };

    // Calculate modal area (centered, 80% width, 60% height)
    let area = frame.area();
    let modal_width = (area.width as f32 * 0.8) as u16;
    let modal_height = (area.height as f32 * 0.6) as u16;
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    // Clear the area behind the modal
    frame.render_widget(Clear, modal_area);

    // Get type label and color
    let (type_label, color) = match entry.event_type {
        LogEventType::Info => ("Info", Color::Blue),
        LogEventType::Thinking => ("Thinking", Color::Magenta),
        LogEventType::ToolUse => ("Tool Use", Color::Cyan),
        LogEventType::ToolResult => ("Tool Result", Color::Green),
        LogEventType::Text => ("Output", Color::White),
        LogEventType::Review => ("Review", Color::Yellow),
        LogEventType::Fix => ("Fix", Color::Cyan),
        LogEventType::Error => ("Error", Color::Red),
    };

    let title = format!(" {} - {} ", type_label, entry.timestamp);

    // Build content with word wrap
    let content = Paragraph::new(entry.message.clone())
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_bottom(Line::from(" Press Esc/Enter/q to close ").centered())
                .border_style(Style::default().fg(color)),
        );

    frame.render_widget(content, modal_area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &AiRallyState) {
    let help_text = if state.showing_log_detail {
        "Esc/Enter/q: Close detail"
    } else {
        match state.state {
            RallyState::WaitingForClarification => {
                "y: Open editor | n: Skip | j/k/↑↓: select | Enter: detail | q: Abort"
            }
            RallyState::WaitingForPermission => {
                "y: Approve | n: Deny | j/k/↑↓: select | Enter: detail | q: Abort"
            }
            RallyState::Completed => "j/k/↑↓: select | Enter: detail | b: Background | q: Close",
            RallyState::Aborted => "j/k/↑↓: select | Enter: detail | b: Background | q: Close",
            RallyState::Error => {
                "r: Retry | j/k/↑↓: select | Enter: detail | b: Background | q: Close"
            }
            _ => "j/k/↑↓: select | Enter: detail | b: Background | q: Abort",
        }
    };

    let status_bar = Paragraph::new(Line::from(vec![Span::styled(
        help_text,
        Style::default().fg(Color::Cyan),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(status_bar, area);
}

fn truncate_string(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
