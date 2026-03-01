use anyhow::Result;
use crossterm::event::{self, KeyCode, KeyEvent};
use tokio::sync::mpsc;

use crate::github;
use crate::loader::CommentSubmitResult;
use crate::ui::text_area::TextAreaAction;

use super::types::*;
use super::App;

impl App {
    pub(crate) fn handle_text_input(&mut self, key: event::KeyEvent) -> Result<()> {
        // 送信中は入力を無視
        if self.comment_submitting {
            return Ok(());
        }

        match self.input_text_area.input(key) {
            TextAreaAction::Submit => {
                let content = self.input_text_area.content();
                if content.trim().is_empty() {
                    // 空の場合はキャンセル扱い
                    self.cancel_input();
                    return Ok(());
                }

                match self.input_mode.take() {
                    Some(InputMode::Comment(ctx)) => {
                        self.submit_comment(ctx, content);
                    }
                    Some(InputMode::Suggestion {
                        context,
                        original_code: _,
                    }) => {
                        self.submit_suggestion(context, content);
                    }
                    Some(InputMode::Reply { comment_id, .. }) => {
                        self.submit_reply(comment_id, content);
                    }
                    None => {}
                }
                self.state = self.preview_return_state;
            }
            TextAreaAction::Cancel => {
                self.cancel_input();
            }
            TextAreaAction::Continue => {}
            TextAreaAction::PendingSequence => {
                // Waiting for more keys in a sequence, do nothing
            }
        }
        Ok(())
    }
    pub(crate) fn cancel_input(&mut self) {
        self.input_mode = None;
        self.input_text_area.clear();
        self.state = self.preview_return_state;
    }
    pub(crate) fn submit_comment(&mut self, ctx: LineInputContext, body: String) {
        let Some(file) = self.files().get(ctx.file_index) else {
            return;
        };
        let Some(pr) = self.pr() else {
            return;
        };

        let commit_id = pr.head.sha.clone();
        let filename = file.filename.clone();
        let repo = self.repo.clone();
        let pr_number = self.pr_number();
        let position = ctx.diff_position;
        let start_line = ctx.start_line_number;
        let end_line = ctx.line_number;

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some((pr_number, rx));
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = if let Some(start) = start_line {
                github::create_multiline_review_comment(
                    &repo, pr_number, &commit_id, &filename, start, end_line, "RIGHT", &body,
                )
                .await
            } else {
                github::create_review_comment(
                    &repo, pr_number, &commit_id, &filename, position, &body,
                )
                .await
            };

            let _ = tx
                .send(match result {
                    Ok(_) => CommentSubmitResult::Success,
                    Err(e) => CommentSubmitResult::Error(e.to_string()),
                })
                .await;
        });
    }

    pub(crate) fn submit_suggestion(&mut self, ctx: LineInputContext, suggested_code: String) {
        let Some(file) = self.files().get(ctx.file_index) else {
            return;
        };
        let Some(pr) = self.pr() else {
            return;
        };

        let commit_id = pr.head.sha.clone();
        let filename = file.filename.clone();
        let body = format!("```suggestion\n{}\n```", suggested_code.trim_end());
        let repo = self.repo.clone();
        let pr_number = self.pr_number();
        let position = ctx.diff_position;
        let start_line = ctx.start_line_number;
        let end_line = ctx.line_number;

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some((pr_number, rx));
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = if let Some(start) = start_line {
                github::create_multiline_review_comment(
                    &repo, pr_number, &commit_id, &filename, start, end_line, "RIGHT", &body,
                )
                .await
            } else {
                github::create_review_comment(
                    &repo, pr_number, &commit_id, &filename, position, &body,
                )
                .await
            };

            let _ = tx
                .send(match result {
                    Ok(_) => CommentSubmitResult::Success,
                    Err(e) => CommentSubmitResult::Error(e.to_string()),
                })
                .await;
        });
    }

    pub(crate) fn submit_reply(&mut self, comment_id: u64, body: String) {
        let repo = self.repo.clone();
        let pr_number = self.pr_number();

        let (tx, rx) = mpsc::channel(1);
        self.comment_submit_receiver = Some((pr_number, rx));
        self.comment_submitting = true;

        tokio::spawn(async move {
            let result = github::create_reply_comment(&repo, pr_number, comment_id, &body).await;

            let _ = tx
                .send(match result {
                    Ok(_) => CommentSubmitResult::Success,
                    Err(e) => CommentSubmitResult::Error(e.to_string()),
                })
                .await;
        });
    }
    pub(crate) fn handle_pending_approve_choice(&mut self, key: &KeyEvent) -> PendingApproveChoice {
        if self.pending_approve_body.is_none() {
            return PendingApproveChoice::Ignore;
        }
        if self.matches_single_key(key, &self.config.keybindings.approve) {
            PendingApproveChoice::Submit
        } else if self.matches_single_key(key, &self.config.keybindings.quit)
            || key.code == KeyCode::Esc
        {
            self.pending_approve_body = None;
            self.submission_result = None;
            self.submission_result_time = None;
            PendingApproveChoice::Cancel
        } else {
            PendingApproveChoice::Ignore
        }
    }
}
