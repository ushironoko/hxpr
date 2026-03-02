use std::collections::HashSet;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs},
    Frame,
};

use crate::ai::{PromptLoader, PromptSource};
use crate::app::{App, HelpTab};
use crate::config::{Config, KeybindingsConfig};
use crate::syntax::available_themes;

/// Format a key display with padding for alignment
fn fmt_key(key: &str, width: usize) -> String {
    format!("  {:<width$}", key, width = width)
}

/// Format a label with padding for alignment (used in config tab)
fn fmt_label(label: &str, width: usize) -> String {
    format!("  {:<width$}", label, width = width)
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab header
            Constraint::Min(0),   // Content
            Constraint::Length(1), // Footer
        ])
        .split(frame.area());

    // Tab header
    render_tab_header(frame, app, chunks[0]);

    // Content
    match app.help_tab {
        HelpTab::Keybindings => render_keybindings_tab(frame, app, chunks[1]),
        HelpTab::Config => render_config_tab(frame, app, chunks[1]),
    }

    // Footer
    render_help_footer(frame, app, chunks[2]);
}

fn render_tab_header(frame: &mut Frame, app: &App, area: Rect) {
    let keybindings_style = if app.help_tab == HelpTab::Keybindings {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let config_style = if app.help_tab == HelpTab::Config {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let selected = match app.help_tab {
        HelpTab::Keybindings => 0,
        HelpTab::Config => 1,
    };

    let titles = vec![
        Line::from(Span::styled("Keybindings", keybindings_style)),
        Line::from(Span::styled("Config", config_style)),
    ];

    let tabs = Tabs::new(titles)
        .select(selected)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help ([/]: switch tab)"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn render_keybindings_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let kb = &app.config.keybindings;
    let help_lines = build_help_lines(kb);
    let total_lines = help_lines.len();
    let content_height = area.height.saturating_sub(2) as usize;

    let max_scroll = total_lines.saturating_sub(content_height);
    if app.help_scroll_offset > max_scroll {
        app.help_scroll_offset = max_scroll;
    }

    let scroll_info = if total_lines > content_height {
        format!(" ({}/{})", app.help_scroll_offset + 1, max_scroll + 1)
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
    frame.render_widget(help, area);

    if total_lines > content_height {
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll + 1).position(app.help_scroll_offset);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
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

fn render_config_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let config_lines = build_config_lines(&app.config);
    let total_lines = config_lines.len();
    let content_height = area.height.saturating_sub(2) as usize;

    let max_scroll = total_lines.saturating_sub(content_height);
    if app.config_scroll_offset > max_scroll {
        app.config_scroll_offset = max_scroll;
    }

    let scroll_info = if total_lines > content_height {
        format!(" ({}/{})", app.config_scroll_offset + 1, max_scroll + 1)
    } else {
        String::new()
    };

    let config = Paragraph::new(config_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Config{}", scroll_info)),
        )
        .scroll((app.config_scroll_offset as u16, 0));
    frame.render_widget(config, area);

    if total_lines > content_height {
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll + 1).position(app.config_scroll_offset);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
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

fn render_help_footer(frame: &mut Frame, app: &App, area: Rect) {
    let kb = &app.config.keybindings;
    let footer_text = format!(
        " {}/{}: close | [/]: switch tab | j/k: scroll | g/G: top/bottom",
        kb.quit.display(),
        kb.help.display()
    );
    let footer = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(footer, area);
}

/// Get the set of config keys overridden by the local .octorus/config.toml.
/// Uses the pre-computed `local_overrides` from Config (populated at load time)
/// to avoid disk I/O during rendering.
fn get_local_overrides(config: &Config) -> &HashSet<String> {
    &config.local_overrides
}

/// Build a config value line with optional "(local)" override marker.
fn config_value_line(label: &str, value: &str, key: &str, overrides: &HashSet<String>) -> Line<'static> {
    let label_width = 20;
    if overrides.contains(key) {
        Line::from(vec![
            Span::raw(format!("{}{}", fmt_label(label, label_width), value)),
            Span::styled(" (local)", Style::default().fg(Color::Cyan)),
        ])
    } else {
        Line::from(format!("{}{}", fmt_label(label, label_width), value))
    }
}

pub fn build_config_lines(config: &Config) -> Vec<Line<'static>> {
    let label_width = 20;
    let overrides = get_local_overrides(config);

    let global_config_path = Config::config_path();
    let global_status = if config.loaded_global_config.is_some() {
        format!("{} [loaded]", global_config_path.display())
    } else {
        format!("{} [not found]", global_config_path.display())
    };

    let local_config_path = config.project_root.join(".octorus/config.toml");
    let local_status = if config.loaded_local_config.is_some() {
        if overrides.is_empty() {
            format!("{} [loaded]", local_config_path.display())
        } else {
            format!("{} [loaded, overrides global]", local_config_path.display())
        }
    } else {
        format!("{} [not found]", local_config_path.display())
    };

    let editor_display = config
        .editor
        .as_deref()
        .unwrap_or("(default: $EDITOR)")
        .to_string();

    let prompt_dir_display = config
        .ai
        .prompt_dir
        .as_deref()
        .unwrap_or("(default)")
        .to_string();

    // Resolve prompt sources
    let prompt_loader = PromptLoader::new(&config.ai, &config.project_root);
    let prompt_sources = prompt_loader.resolve_all_sources();

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Config Files",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!(
            "{}{}",
            fmt_label("Global", label_width),
            global_status
        )),
        Line::from(format!(
            "{}{}",
            fmt_label("Local", label_width),
            local_status
        )),
        Line::from(format!(
            "{}{}",
            fmt_label("Project root", label_width),
            config.project_root.display()
        )),
    ];

    // Show stripped keys warning
    if !config.local_config_stripped_keys.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "  Blocked local overrides (security):",
            Style::default().fg(Color::Red),
        )]));
        for key in &config.local_config_stripped_keys {
            lines.push(Line::from(vec![Span::styled(
                format!("    {} (ignored)", key),
                Style::default().fg(Color::Red),
            )]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Diff Settings",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(config_value_line("Theme", &config.diff.theme, "diff.theme", overrides));
    lines.push(config_value_line("Tab width", &config.diff.tab_width.to_string(), "diff.tab_width", overrides));
    lines.push(config_value_line("Background color", &config.diff.bg_color.to_string(), "diff.bg_color", overrides));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Editor",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(config_value_line("Editor", &editor_display, "editor", overrides));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "AI Rally Settings",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(config_value_line("Reviewer", &config.ai.reviewer, "ai.reviewer", overrides));
    lines.push(config_value_line("Reviewee", &config.ai.reviewee, "ai.reviewee", overrides));
    lines.push(config_value_line("Max iterations", &config.ai.max_iterations.to_string(), "ai.max_iterations", overrides));
    lines.push(config_value_line("Timeout (secs)", &config.ai.timeout_secs.to_string(), "ai.timeout_secs", overrides));
    lines.push(config_value_line("Auto post", &config.ai.auto_post.to_string(), "ai.auto_post", overrides));
    lines.push(config_value_line("Prompt dir", &prompt_dir_display, "ai.prompt_dir", overrides));

    // Reviewer additional tools
    if !config.ai.reviewer_additional_tools.is_empty() {
        lines.push(config_value_line(
            "Reviewer tools",
            &config.ai.reviewer_additional_tools.join(", "),
            "ai.reviewer_additional_tools",
            overrides,
        ));
    }

    // Reviewee additional tools
    if !config.ai.reviewee_additional_tools.is_empty() {
        lines.push(config_value_line(
            "Reviewee tools",
            &config.ai.reviewee_additional_tools.join(", "),
            "ai.reviewee_additional_tools",
            overrides,
        ));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Prompt Resolution",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));

    for (filename, source) in prompt_sources {
        let source_display = match source {
            PromptSource::Local(path) => format!("local ({})", path.display()),
            PromptSource::PromptDir(path) => format!("prompt_dir ({})", path.display()),
            PromptSource::Global(path) => format!("global ({})", path.display()),
            PromptSource::Embedded => "embedded default".to_string(),
        };
        lines.push(Line::from(format!(
            "{}{}",
            fmt_label(&filename, label_width),
            source_display
        )));
    }

    lines.push(Line::from(""));

    lines
}

fn build_help_lines(kb: &KeybindingsConfig) -> Vec<Line<'static>> {
    let key_width = 14; // Width for key column

    vec![
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
            fmt_key(
                &format!("{}/Shift+Enter", kb.multiline_select.display()),
                key_width,
            )
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
            fmt_key(&kb.comment.display(), key_width)
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_config_lines_does_not_panic() {
        let config = Config::default();
        let lines = build_config_lines(&config);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_build_config_lines_contains_sections() {
        let config = Config::default();
        let lines = build_config_lines(&config);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let joined = text.join("\n");

        assert!(joined.contains("Config Files"));
        assert!(joined.contains("Diff Settings"));
        assert!(joined.contains("Editor"));
        assert!(joined.contains("AI Rally Settings"));
        assert!(joined.contains("Prompt Resolution"));
    }

    #[test]
    fn test_build_config_lines_shows_default_values() {
        let config = Config::default();
        let lines = build_config_lines(&config);
        let text: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let joined = text.join("\n");

        assert!(joined.contains("base16-ocean.dark"));
        assert!(joined.contains("claude"));
        // Prompt resolution should show some source (embedded or global depending on env)
        assert!(
            joined.contains("reviewer.md"),
            "Should contain prompt filename"
        );
    }
}
