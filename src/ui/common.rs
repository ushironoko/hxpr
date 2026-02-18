use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame,
};

use crate::ai::RallyState;
use crate::app::{App, DataState};

/// Build PR info string for header display (shared between file_list and ai_rally)
pub fn build_pr_info(app: &App) -> String {
    if app.is_local_mode() {
        let af = if app.is_local_auto_focus() { " AF" } else { "" };
        format!("[LOCAL{}] Local HEAD diff", af)
    } else {
        match &app.data_state {
            DataState::Loaded { pr, .. } => {
                format!("PR #{}: {} by @{}", pr.number, pr.title, pr.user.login)
            }
            _ => match app.pr_number {
                Some(n) => format!("PR #{}", n),
                None => "PR".to_string(),
            },
        }
    }
}

/// Render rally status bar for background rally indication
pub fn render_rally_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let Some(rally_state) = &app.ai_rally_state else {
        return;
    };

    let (text, color) = match rally_state.state {
        RallyState::Initializing => ("Initializing...", Color::Blue),
        RallyState::ReviewerReviewing => ("Reviewer reviewing...", Color::Yellow),
        RallyState::RevieweeFix => ("Reviewee fixing...", Color::Cyan),
        RallyState::WaitingForClarification => ("Waiting for clarification", Color::Magenta),
        RallyState::WaitingForPermission => ("Waiting for permission", Color::Magenta),
        RallyState::Completed => ("Completed!", Color::Green),
        RallyState::Aborted => ("Aborted - Press A to view", Color::Yellow),
        RallyState::Error => ("Error - Press A to view", Color::Red),
    };

    let status = format!(
        " [Rally: {} ({}/{})] ",
        text, rally_state.iteration, rally_state.max_iterations
    );

    let bar = Paragraph::new(status)
        .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    frame.render_widget(bar, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::github::{Branch, PullRequest, User};

    #[test]
    fn test_build_pr_info_loaded() {
        let mut app = App::new_for_test();
        app.data_state = DataState::Loaded {
            pr: Box::new(PullRequest {
                number: 42,
                title: "Add feature X".to_string(),
                body: None,
                state: "open".to_string(),
                head: Branch {
                    ref_name: "feature".to_string(),
                    sha: "abc".to_string(),
                },
                base: Branch {
                    ref_name: "main".to_string(),
                    sha: "def".to_string(),
                },
                user: User {
                    login: "alice".to_string(),
                },
                updated_at: "2024-01-01T00:00:00Z".to_string(),
            }),
            files: vec![],
        };
        assert_eq!(build_pr_info(&app), "PR #42: Add feature X by @alice");
    }

    #[test]
    fn test_build_pr_info_loading_with_pr_number() {
        let mut app = App::new_for_test();
        app.data_state = DataState::Loading;
        app.pr_number = Some(99);
        assert_eq!(build_pr_info(&app), "PR #99");
    }

    #[test]
    fn test_build_pr_info_loading_without_pr_number() {
        let mut app = App::new_for_test();
        app.data_state = DataState::Loading;
        app.pr_number = None;
        assert_eq!(build_pr_info(&app), "PR");
    }

    #[test]
    fn test_build_pr_info_local_mode() {
        let mut app = App::new_for_test();
        app.set_local_mode(true);
        assert_eq!(build_pr_info(&app), "[LOCAL] Local HEAD diff");
    }

    #[test]
    fn test_build_pr_info_local_mode_with_auto_focus() {
        let mut app = App::new_for_test();
        app.set_local_mode(true);
        app.set_local_auto_focus(true);
        assert_eq!(build_pr_info(&app), "[LOCAL AF] Local HEAD diff");
    }
}
