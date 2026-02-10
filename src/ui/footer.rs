use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::app::App;

/// Build footer line content based on app state.
///
/// During submission or result display, the footer shows only the status
/// (full-width override). Otherwise, it shows the normal help text with
/// optional comments loading indicator appended.
pub fn build_footer_line<'a>(app: &'a App, help_text: &'a str) -> Line<'a> {
    if app.is_submitting_comment() {
        Line::from(Span::styled(
            format!("{} Submitting...", app.spinner_char()),
            Style::default().fg(Color::Yellow),
        ))
    } else if let Some((success, message)) = &app.submission_result {
        let (icon, color) = if *success {
            ("\u{2713}", Color::Green)
        } else {
            ("\u{2717}", Color::Red)
        };
        Line::from(Span::styled(
            format!("{} {}", icon, message),
            Style::default().fg(color),
        ))
    } else {
        let mut spans = vec![Span::raw(help_text)];
        if app.comments_loading {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("{} Loading comments...", app.spinner_char()),
                Style::default().fg(Color::Yellow),
            ));
        }
        Line::from(spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    const HELP: &str = "j/k: move | q: quit";

    fn line_to_string(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn test_idle_shows_help_text_only() {
        let app = App::new_for_test();
        let line = build_footer_line(&app, HELP);
        let text = line_to_string(&line);
        assert_eq!(text, HELP);
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn test_comments_loading_appends_indicator() {
        let mut app = App::new_for_test();
        app.comments_loading = true;
        let line = build_footer_line(&app, HELP);
        let text = line_to_string(&line);
        assert!(text.starts_with(HELP));
        assert!(text.contains("Loading comments..."));
        assert_eq!(line.spans.len(), 3); // help + "  " + loading
    }

    #[test]
    fn test_submitting_shows_status_only() {
        let mut app = App::new_for_test();
        app.set_submitting_for_test(true);
        let line = build_footer_line(&app, HELP);
        let text = line_to_string(&line);
        assert!(text.contains("Submitting..."));
        assert!(!text.contains(HELP));
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn test_success_result_shows_checkmark_only() {
        let mut app = App::new_for_test();
        app.submission_result = Some((true, "Submitted".to_string()));
        let line = build_footer_line(&app, HELP);
        let text = line_to_string(&line);
        assert!(text.contains("\u{2713}"));
        assert!(text.contains("Submitted"));
        assert!(!text.contains(HELP));
        assert_eq!(line.spans.len(), 1);

        // Check color is green
        let style = line.spans[0].style;
        assert_eq!(style.fg, Some(Color::Green));
    }

    #[test]
    fn test_error_result_shows_cross_only() {
        let mut app = App::new_for_test();
        app.submission_result = Some((false, "Failed: network error".to_string()));
        let line = build_footer_line(&app, HELP);
        let text = line_to_string(&line);
        assert!(text.contains("\u{2717}"));
        assert!(text.contains("Failed: network error"));
        assert!(!text.contains(HELP));
        assert_eq!(line.spans.len(), 1);

        // Check color is red
        let style = line.spans[0].style;
        assert_eq!(style.fg, Some(Color::Red));
    }
}
