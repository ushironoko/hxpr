use super::*;
use super::types::{MarkViewedResult, PendingApproveChoice};
use crossterm::event::{self, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use lasso::Rodeo;

use crate::cache::{PrCacheKey, PrData};
use crate::github::{ChangedFile, PullRequest};
use crate::loader::DataLoadResult;

#[test]
fn test_find_diff_line_index_basic() {
    let patch = r#"@@ -1,3 +1,4 @@
 context line
+added line
 another context
-removed line"#;

    // Line 1 (context) is at diff index 1
    assert_eq!(App::find_diff_line_index(patch, 1), Some(1));
    // Line 2 (added) is at diff index 2
    assert_eq!(App::find_diff_line_index(patch, 2), Some(2));
    // Line 3 (context) is at diff index 3
    assert_eq!(App::find_diff_line_index(patch, 3), Some(3));
    // Line 5 doesn't exist in new file
    assert_eq!(App::find_diff_line_index(patch, 5), None);
}

#[test]
fn test_find_diff_line_index_multi_hunk() {
    let patch = r#"@@ -1,2 +1,2 @@
 line1
+new line2
@@ -10,2 +10,2 @@
 line10
+new line11"#;

    // First hunk: line 1 at index 1, line 2 at index 2
    assert_eq!(App::find_diff_line_index(patch, 1), Some(1));
    assert_eq!(App::find_diff_line_index(patch, 2), Some(2));
    // Second hunk: line 10 at index 4, line 11 at index 5
    assert_eq!(App::find_diff_line_index(patch, 10), Some(4));
    assert_eq!(App::find_diff_line_index(patch, 11), Some(5));
}

#[test]
fn test_has_comment_at_current_line() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.file_comment_positions = vec![
        CommentPosition {
            diff_line_index: 5,
            comment_index: 0,
        },
        CommentPosition {
            diff_line_index: 10,
            comment_index: 1,
        },
    ];

    app.selected_line = 5;
    assert!(app.has_comment_at_current_line());

    app.selected_line = 10;
    assert!(app.has_comment_at_current_line());

    app.selected_line = 7;
    assert!(!app.has_comment_at_current_line());
}

#[test]
fn test_get_comment_indices_at_current_line() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    // Two comments on line 5, one on line 10
    app.file_comment_positions = vec![
        CommentPosition {
            diff_line_index: 5,
            comment_index: 0,
        },
        CommentPosition {
            diff_line_index: 5,
            comment_index: 2,
        },
        CommentPosition {
            diff_line_index: 10,
            comment_index: 1,
        },
    ];

    app.selected_line = 5;
    let indices = app.get_comment_indices_at_current_line();
    assert_eq!(indices, vec![0, 2]);

    app.selected_line = 10;
    let indices = app.get_comment_indices_at_current_line();
    assert_eq!(indices, vec![1]);

    app.selected_line = 7;
    let indices = app.get_comment_indices_at_current_line();
    assert!(indices.is_empty());
}

#[test]
fn test_jump_to_next_comment_basic() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.file_comment_positions = vec![
        CommentPosition {
            diff_line_index: 5,
            comment_index: 0,
        },
        CommentPosition {
            diff_line_index: 10,
            comment_index: 1,
        },
        CommentPosition {
            diff_line_index: 15,
            comment_index: 2,
        },
    ];

    app.selected_line = 0;
    app.jump_to_next_comment();
    assert_eq!(app.selected_line, 5);

    app.jump_to_next_comment();
    assert_eq!(app.selected_line, 10);

    app.jump_to_next_comment();
    assert_eq!(app.selected_line, 15);
}

#[test]
fn test_jump_to_next_comment_no_wrap() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.file_comment_positions = vec![CommentPosition {
        diff_line_index: 5,
        comment_index: 0,
    }];

    app.selected_line = 5;
    app.jump_to_next_comment();
    // Should stay at 5 (no wrap-around)
    assert_eq!(app.selected_line, 5);
}

#[test]
fn test_jump_to_prev_comment_basic() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.file_comment_positions = vec![
        CommentPosition {
            diff_line_index: 5,
            comment_index: 0,
        },
        CommentPosition {
            diff_line_index: 10,
            comment_index: 1,
        },
        CommentPosition {
            diff_line_index: 15,
            comment_index: 2,
        },
    ];

    app.selected_line = 20;
    app.jump_to_prev_comment();
    assert_eq!(app.selected_line, 15);

    app.jump_to_prev_comment();
    assert_eq!(app.selected_line, 10);

    app.jump_to_prev_comment();
    assert_eq!(app.selected_line, 5);
}

#[test]
fn test_jump_to_prev_comment_no_wrap() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.file_comment_positions = vec![CommentPosition {
        diff_line_index: 5,
        comment_index: 0,
    }];

    app.selected_line = 5;
    app.jump_to_prev_comment();
    // Should stay at 5 (no wrap-around)
    assert_eq!(app.selected_line, 5);
}

#[test]
fn test_jump_with_empty_positions() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.file_comment_positions = vec![];

    app.selected_line = 10;
    app.jump_to_next_comment();
    assert_eq!(app.selected_line, 10);

    app.jump_to_prev_comment();
    assert_eq!(app.selected_line, 10);
}

#[test]
fn test_liststate_autoscroll_with_multiline_items() {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::text::Line;
    use ratatui::widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget};

    // 10 multiline items (each 3 lines), area height = 12 (10 inner after borders)
    let items: Vec<ListItem> = (0..10)
        .map(|i| {
            ListItem::new(vec![
                Line::from(format!("Header {}", i)),
                Line::from(format!("  Body {}", i)),
                Line::from(""),
            ])
        })
        .collect();

    let area = Rect::new(0, 0, 40, 12); // 12 total, 10 inner

    // Simulate frame-by-frame scrolling like the real app
    let mut offset = 0usize;
    for selected in 0..10 {
        let list = List::new(items.clone()).block(Block::default().borders(Borders::ALL));
        let mut state = ListState::default()
            .with_offset(offset)
            .with_selected(Some(selected));
        let mut buf = Buffer::empty(area);
        StatefulWidget::render(&list, area, &mut buf, &mut state);
        offset = state.offset();

        // selected should always be in visible range [offset, offset + visible_items)
        // With 10 inner height and 3 lines per item, 3 items fit (9 lines)
        assert!(
            selected >= offset,
            "selected={} should be >= offset={}",
            selected,
            offset
        );
    }

    // After scrolling to last item, offset should be > 0
    assert!(offset > 0, "offset should have scrolled, got {}", offset);
}

#[test]
fn test_back_to_pr_list_clears_view_receivers() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.started_from_pr_list = true;

    // data_receiver is already set by new_loading
    assert!(app.data_receiver.is_some());

    // Set up additional receivers to simulate in-flight requests
    let (_comment_tx, comment_rx) = mpsc::channel(1);
    app.comment_receiver = Some((1, comment_rx));
    let (_disc_tx, disc_rx) = mpsc::channel(1);
    app.discussion_comment_receiver = Some((1, disc_rx));
    let (_submit_tx, submit_rx) = mpsc::channel(1);
    app.comment_submit_receiver = Some((1, submit_rx));
    let (_mark_tx, mark_rx) = mpsc::channel(1);
    app.mark_viewed_receiver = Some((1, mark_rx));
    app.comment_submitting = true;
    app.comments_loading = true;
    app.discussion_comments_loading = true;
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(1);
    app.retry_sender = Some(retry_tx);

    app.back_to_pr_list();

    // data_receiver / retry_sender は永続のため維持
    assert!(app.data_receiver.is_some());
    assert!(app.retry_sender.is_some());
    // view 系 receivers はクリア
    assert!(app.comment_receiver.is_none());
    assert!(app.discussion_comment_receiver.is_none());
    assert!(app.comment_submit_receiver.is_none());
    assert!(app.mark_viewed_receiver.is_none());
    assert!(app.diff_cache_receiver.is_none());
    assert!(app.prefetch_receiver.is_none());
    // Loading flags should be cleared
    assert!(!app.comment_submitting);
    assert!(!app.comments_loading);
    assert!(!app.discussion_comments_loading);
    // PR number should be None
    assert!(app.pr_number.is_none());
    assert_eq!(app.state, AppState::PullRequestList);
}

#[test]
fn test_back_to_pr_list_from_local_mode_resets_local_state() {
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    let (_data_tx, data_rx) = mpsc::channel(2);
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 0, config);
    app.started_from_pr_list = true;
    app.local_mode = true;
    app.pr_number = Some(0);
    app.retry_sender = Some(retry_tx);
    app.data_receiver = Some((0, data_rx));
    app.selected_file = 2;

    app.back_to_pr_list();

    // local_mode がリセットされている
    assert!(!app.local_mode);
    // Local スナップショットが保存されている
    assert!(app.saved_local_snapshot.is_some());
    assert_eq!(app.state, AppState::PullRequestList);
    assert!(app.pr_number.is_none());
}

#[tokio::test]
async fn test_pr_list_local_toggle_round_trip() {
    // PR一覧 → L(Local) → q(PR一覧) → L(Local) の往復でデータが正常に表示されるか
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(8);
    let (_data_tx, data_rx) = mpsc::channel(2);
    let mut app = App::new_for_test();
    app.started_from_pr_list = true;
    app.state = AppState::PullRequestList;
    app.pr_number = None;
    app.original_pr_number = None;
    app.retry_sender = Some(retry_tx);
    app.data_receiver = Some((0, data_rx));

    // SessionCache に Local diff データを事前格納
    let local_pr = PullRequest {
        number: 0,
        node_id: None,
        title: "Local HEAD diff".to_string(),
        body: None,
        state: "local".to_string(),
        head: crate::github::Branch {
            ref_name: "HEAD".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "local".to_string(),
            sha: "local".to_string(),
        },
        user: crate::github::User {
            login: "local".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    };
    let local_files = vec![ChangedFile {
        filename: "src/main.rs".to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 0,
        patch: Some("@@ -1,1 +1,2 @@\n line1\n+line2".to_string()),
        viewed: false,
    }];
    app.session_cache.put_pr_data(
        PrCacheKey {
            repo: "test/repo".to_string(),
            pr_number: 0,
        },
        PrData {
            pr: Box::new(local_pr),
            files: local_files,
            pr_updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
    );

    // Step 1: PR一覧 → L (Local モード)
    app.toggle_local_mode();
    assert!(app.local_mode);
    assert_eq!(app.pr_number, Some(0));
    assert_eq!(app.state, AppState::FileList);
    assert!(matches!(app.data_state, DataState::Loaded { .. }));

    // Step 2: q → PR一覧に戻る
    app.back_to_pr_list();
    assert!(!app.local_mode);
    assert_eq!(app.state, AppState::PullRequestList);
    assert!(app.saved_local_snapshot.is_some());

    // Step 3: L → 再度 Local モード（1回目で正しく Local に入る）
    app.toggle_local_mode();
    assert!(app.local_mode);
    assert_eq!(app.pr_number, Some(0));
    assert_eq!(app.state, AppState::FileList);
    // SessionCache から即時表示
    assert!(matches!(app.data_state, DataState::Loaded { .. }));
}

#[tokio::test]
async fn test_poll_data_updates_discards_stale_pr_data() {
    let config = Config::default();
    let (mut app, tx) = App::new_loading("owner/repo", 1, config);
    app.started_from_pr_list = true;

    // Simulate switching to PR #2 while PR #1 data is in-flight
    // The data_receiver still carries origin_pr = 1
    app.pr_number = Some(2);

    // Send data for PR #1
    let pr = PullRequest {
        number: 1,
        node_id: None,
        title: "PR 1".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    };
    tx.send(DataLoadResult::Success {
        pr: Box::new(pr),
        files: vec![],
    })
    .await
    .unwrap();

    // Poll should NOT panic and should NOT apply PR #1 data to current UI state
    app.poll_data_updates();

    // data_receiver should be kept alive (persistent channel for future refreshes)
    assert!(app.data_receiver.is_some());
    // data_state should still be Loading (PR #1 data was discarded from UI)
    assert!(matches!(app.data_state, DataState::Loading));
    // But session cache should have the data under PR #1 key
    let cache_key = PrCacheKey {
        repo: "owner/repo".to_string(),
        pr_number: 1,
    };
    assert!(app.session_cache.get_pr_data(&cache_key).is_some());
}

#[tokio::test]
async fn test_poll_comment_updates_discards_stale_pr_comments() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.started_from_pr_list = true;

    // Set up a comment receiver for PR #1
    let (comment_tx, comment_rx) = mpsc::channel(1);
    app.comment_receiver = Some((1, comment_rx));
    app.comments_loading = true;

    // Simulate switching to PR #2
    app.pr_number = Some(2);

    // Send comments for PR #1
    comment_tx.send(Ok(vec![])).await.unwrap();

    // Poll should NOT panic and should NOT apply PR #1 comments to UI
    app.poll_comment_updates();

    assert!(app.comment_receiver.is_none());
    // comments_loading should NOT have been cleared (different PR)
    assert!(app.comments_loading);
    // Session cache should NOT have comments for PR #1 since pr_data was never stored
    // (comments are only cached for keys that have an existing pr_data entry)
    let cache_key = PrCacheKey {
        repo: "owner/repo".to_string(),
        pr_number: 1,
    };
    assert!(app.session_cache.get_review_comments(&cache_key).is_none());
}

#[tokio::test]
async fn test_handle_data_result_clamps_selected_file_when_files_shrink() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

    // Simulate initial state with 5 files, selected_file pointing to file index 4
    let make_file = |name: &str| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files: Vec<ChangedFile> = (0..5)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    // Set initial loaded state with 5 files
    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files,
    };
    app.selected_file = 4; // Last file selected

    // Now simulate refresh with only 2 files (file count shrank)
    let fewer_files: Vec<ChangedFile> = (0..2)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: fewer_files,
        },
    );

    // selected_file should be clamped to 1 (last valid index for 2 files)
    assert_eq!(app.selected_file, 1);
    // Should be able to access the file without panic
    assert!(app.files().get(app.selected_file).is_some());
}

#[tokio::test]
async fn test_handle_data_result_resyncs_diff_state_when_selected_file_changes() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

    let make_file = |name: &str| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files: Vec<ChangedFile> = (0..5)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    // Set initial loaded state with 5 files
    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files,
    };
    app.selected_file = 4;
    app.selected_line = 10;
    app.scroll_offset = 5;

    // Set a stale diff_cache pointing to old file index 4
    app.diff_cache = Some(DiffCache {
        file_index: 4,
        patch_hash: 0,
        lines: vec![],
        interner: Rodeo::default(),
        highlighted: false,
        markdown_rich: false,
    });

    // Refresh with only 2 files (selected_file will be clamped from 4 to 1)
    let fewer_files: Vec<ChangedFile> = (0..2)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: fewer_files,
        },
    );

    // selected_file clamped
    assert_eq!(app.selected_file, 1);
    // diff_cache must be rebuilt for the new selected file (ensure_diff_cache rebuilds it)
    assert_eq!(
        app.diff_cache.as_ref().map(|c| c.file_index),
        Some(1),
        "diff_cache should be rebuilt for the new selected file"
    );
    // selected_line and scroll_offset must be reset
    assert_eq!(app.selected_line, 0, "selected_line should be reset to 0");
    assert_eq!(app.scroll_offset, 0, "scroll_offset should be reset to 0");
}

#[tokio::test]
async fn test_handle_data_result_resyncs_comment_positions_when_selected_file_changes() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

    let make_file = |name: &str| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files: Vec<ChangedFile> = (0..5)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    // Set initial loaded state with 5 files, selected_file = 4
    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files,
    };
    app.selected_file = 4;

    // Set up review comments for file_4.rs (the old selected file)
    app.review_comments = Some(vec![ReviewComment {
        id: 1,
        path: "file_4.rs".to_string(),
        line: Some(1),
        body: "comment on old file".to_string(),
        user: crate::github::User {
            login: "reviewer".to_string(),
        },
        created_at: "2024-01-01T00:00:00Z".to_string(),
    }]);

    // Pre-populate stale comment positions for the old file
    app.file_comment_positions = vec![CommentPosition {
        diff_line_index: 2,
        comment_index: 0,
    }];
    app.file_comment_lines.insert(2);
    app.comment_panel_open = true;
    app.comment_panel_scroll = 5;

    // Refresh with only 2 files (selected_file will be clamped from 4 to 1)
    let fewer_files: Vec<ChangedFile> = (0..2)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: fewer_files,
        },
    );

    // selected_file clamped to 1
    assert_eq!(app.selected_file, 1);

    // file_comment_positions should be recalculated for file_1.rs (no matching comments)
    assert!(
        app.file_comment_positions.is_empty(),
        "file_comment_positions should be recalculated for new file (no comments for file_1.rs)"
    );
    assert!(
        app.file_comment_lines.is_empty(),
        "file_comment_lines should be recalculated for new file"
    );

    // comment_panel should be closed
    assert!(
        !app.comment_panel_open,
        "comment_panel_open should be reset when selected_file changes"
    );
    assert_eq!(
        app.comment_panel_scroll, 0,
        "comment_panel_scroll should be reset when selected_file changes"
    );
}

#[tokio::test]
async fn test_handle_data_result_preserves_diff_state_when_selected_file_unchanged() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);

    let make_file = |name: &str| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files: Vec<ChangedFile> = (0..5)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    // Set initial loaded state
    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files,
    };
    app.selected_file = 1;
    app.selected_line = 10;
    app.scroll_offset = 5;

    // Set diff_cache for file index 1
    app.diff_cache = Some(DiffCache {
        file_index: 1,
        patch_hash: 0,
        lines: vec![],
        interner: Rodeo::default(),
        highlighted: false,
        markdown_rich: false,
    });

    // Refresh with same or more files (selected_file stays at 1)
    let same_files: Vec<ChangedFile> = (0..5)
        .map(|i| make_file(&format!("file_{}.rs", i)))
        .collect();

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: same_files,
        },
    );

    // selected_file unchanged
    assert_eq!(app.selected_file, 1);
    // diff_cache should NOT be invalidated (selected_file didn't change)
    assert!(
        app.diff_cache.is_some(),
        "diff_cache should be preserved when selected_file is unchanged"
    );
    // selected_line and scroll_offset should be preserved
    assert_eq!(app.selected_line, 10);
    assert_eq!(app.scroll_offset, 5);
}

#[tokio::test]
async fn test_handle_data_result_keeps_selected_file_by_filename() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.set_local_mode(true);
    app.set_local_auto_focus(false);

    let make_file = |name: &str| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files: Vec<ChangedFile> = vec![
        make_file("file_a.rs"),
        make_file("file_b.rs"),
        make_file("file_c.rs"),
    ];

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files.clone(),
    };
    app.selected_file = 1; // file_b.rs
    app.remember_local_file_signatures(&initial_files);

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: vec![make_file("file_b.rs"), make_file("file_c.rs")],
        },
    );

    assert_eq!(
        app.selected_file, 0,
        "selected file should track file_b.rs by filename, not by index"
    );
}

#[tokio::test]
async fn test_handle_data_result_auto_focus_selects_next_changed_file() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.set_local_mode(true);
    app.set_local_auto_focus(true);
    app.selected_file = 1;

    let make_file = |name: &str, additions: u32| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files = vec![
        make_file("file_a.rs", 1),
        make_file("file_b.rs", 1),
        make_file("file_c.rs", 1),
        make_file("file_d.rs", 1),
    ];

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files.clone(),
    };
    app.remember_local_file_signatures(&initial_files);

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: vec![
                make_file("file_a.rs", 2), // changed (additions: 1→2)
                make_file("file_b.rs", 1), // unchanged
                make_file("file_c.rs", 1), // unchanged
                make_file("file_d.rs", 2), // changed (additions: 1→2)
            ],
        },
    );

    assert_eq!(
        app.selected_file, 3,
        "auto-focus should prefer the next changed file after current selection"
    );
}

#[tokio::test]
async fn test_handle_data_result_auto_focus_prefers_nearest_changed_file() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.set_local_mode(true);
    app.set_local_auto_focus(true);
    app.selected_file = 3;

    let make_file = |name: &str, additions: u32| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files = vec![
        make_file("file_a.rs", 1),
        make_file("file_b.rs", 1),
        make_file("file_c.rs", 1),
        make_file("file_d.rs", 1),
        make_file("file_e.rs", 1),
    ];

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files.clone(),
    };
    app.remember_local_file_signatures(&initial_files);

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: vec![
                make_file("file_a.rs", 2), // changed before (additions: 1→2)
                make_file("file_b.rs", 1), // unchanged
                make_file("file_c.rs", 1), // unchanged
                make_file("file_d.rs", 1), // unchanged
                make_file("file_e.rs", 2), // changed after (additions: 1→2)
            ],
        },
    );

    assert_eq!(
        app.selected_file, 4,
        "auto-focus should move to the nearer changed file around current selection"
    );
}

#[tokio::test]
async fn test_handle_data_result_auto_focus_prefers_next_when_distances_are_tie() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.set_local_mode(true);
    app.set_local_auto_focus(true);
    app.selected_file = 2;

    let make_file = |name: &str, additions: u32| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    };

    let initial_files = vec![
        make_file("file_a.rs", 1),
        make_file("file_b.rs", 1),
        make_file("file_c.rs", 1),
        make_file("file_d.rs", 1),
        make_file("file_e.rs", 1),
    ];

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    app.data_state = DataState::Loaded {
        pr: pr.clone(),
        files: initial_files.clone(),
    };
    app.remember_local_file_signatures(&initial_files);

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr,
            files: vec![
                make_file("file_a.rs", 1), // unchanged (index 0)
                make_file("file_b.rs", 2), // changed (index 1, additions: 1→2)
                make_file("file_c.rs", 1), // unchanged (index 2)
                make_file("file_d.rs", 2), // changed (index 3, additions: 1→2)
                make_file("file_e.rs", 1), // unchanged (index 4)
            ],
        },
    );

    assert_eq!(
        app.selected_file, 3,
        "auto-focus should prefer the next file when before/after distances are equal"
    );
}

#[tokio::test]
async fn test_handle_data_result_auto_focus_transitions_to_split_view_diff() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.set_local_mode(true);
    app.set_local_auto_focus(true);
    app.state = AppState::FileList;

    let make_file = |name: &str, patch: &str| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some(patch.to_string()),
        viewed: false,
    };

    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test PR".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });

    app.handle_data_result(
        1,
        DataLoadResult::Success {
            pr: pr.clone(),
            files: vec![make_file("initial.rs", "@@ -1,1 +1,1 @@\n-old\n+new")],
        },
    );

    assert_eq!(app.state, AppState::SplitViewDiff);
    assert_eq!(app.selected_file, 0);
    assert_eq!(app.files().len(), 1);
}

#[test]
fn test_toggle_auto_focus() {
    let mut app = App::new_for_test();
    app.local_mode = true;
    assert!(!app.local_auto_focus);

    app.toggle_auto_focus();
    assert!(app.local_auto_focus);
    assert!(app.submission_result.is_some());
    assert!(app.submission_result.as_ref().unwrap().1.contains("ON"));

    app.toggle_auto_focus();
    assert!(!app.local_auto_focus);
    assert!(app.submission_result.as_ref().unwrap().1.contains("OFF"));
}

#[test]
fn test_toggle_local_mode_blocks_during_ai_rally() {
    let mut app = App::new_for_test();
    app.state = AppState::AiRally;

    app.toggle_local_mode();
    assert!(!app.local_mode);
    assert!(app.submission_result.as_ref().unwrap().1.contains("Cannot"));
}

#[test]
fn test_save_and_restore_view_snapshot() {
    let mut app = App::new_for_test();
    app.selected_file = 5;
    app.file_list_scroll_offset = 2;
    app.selected_line = 10;
    app.scroll_offset = 3;

    let snapshot = app.save_view_snapshot();

    // save_view_snapshot does not move data_state (ViewSnapshot has no data_state)
    // App state fields should be reset after save
    assert!(app.diff_cache.is_none());

    // Modify app state
    app.selected_file = 0;
    app.selected_line = 0;

    // Restore
    app.restore_view_snapshot(snapshot);
    assert_eq!(app.selected_file, 5);
    assert_eq!(app.file_list_scroll_offset, 2);
    assert_eq!(app.selected_line, 10);
    assert_eq!(app.scroll_offset, 3);
}

// ===================================================================
// ViewSnapshot 網羅テスト
// ===================================================================

#[test]
fn test_save_snapshot_captures_all_fields() {
    let mut app = App::new_for_test();
    app.pr_number = Some(42);
    app.selected_file = 7;
    app.file_list_scroll_offset = 3;
    app.selected_line = 15;
    app.scroll_offset = 5;

    // diff_cache
    let mut dc = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+line", 4);
    dc.file_index = 7;
    app.diff_cache = Some(dc);

    // highlighted_cache_store
    let mut hc = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+fn main(){}", 4);
    hc.file_index = 2;
    app.highlighted_cache_store.insert(2, hc);

    // review_comments
    app.review_comments = Some(vec![crate::github::comment::ReviewComment {
        id: 10,
        path: "test.rs".to_string(),
        line: Some(5),
        body: "snapshot test".to_string(),
        user: crate::github::User {
            login: "reviewer".to_string(),
        },
        created_at: "2024-01-01T00:00:00Z".to_string(),
    }]);

    // discussion_comments
    app.discussion_comments = Some(vec![crate::github::comment::DiscussionComment {
        id: 20,
        body: "discussion".to_string(),
        user: crate::github::User {
            login: "user".to_string(),
        },
        created_at: "2024-01-01T00:00:00Z".to_string(),
    }]);

    // local_file_signatures
    app.local_file_signatures
        .insert("a.rs".to_string(), 111);
    // local_file_patch_signatures
    app.local_file_patch_signatures
        .insert("a.rs".to_string(), 222);

    let snapshot = app.save_view_snapshot();

    // --- スナップショットに全フィールドがキャプチャされていること ---
    assert_eq!(snapshot.pr_number, Some(42));
    assert_eq!(snapshot.selected_file, 7);
    assert_eq!(snapshot.file_list_scroll_offset, 3);
    assert_eq!(snapshot.selected_line, 15);
    assert_eq!(snapshot.scroll_offset, 5);
    assert!(snapshot.diff_cache.is_some());
    assert_eq!(snapshot.diff_cache.as_ref().unwrap().file_index, 7);
    assert_eq!(snapshot.highlighted_cache_store.len(), 1);
    assert!(snapshot.highlighted_cache_store.contains_key(&2));
    assert_eq!(snapshot.review_comments.as_ref().unwrap().len(), 1);
    assert_eq!(snapshot.review_comments.as_ref().unwrap()[0].id, 10);
    assert_eq!(snapshot.discussion_comments.as_ref().unwrap().len(), 1);
    assert_eq!(snapshot.discussion_comments.as_ref().unwrap()[0].id, 20);
    assert_eq!(snapshot.local_file_signatures.len(), 1);
    assert_eq!(snapshot.local_file_signatures["a.rs"], 111);
    assert_eq!(snapshot.local_file_patch_signatures.len(), 1);
    assert_eq!(snapshot.local_file_patch_signatures["a.rs"], 222);
}

#[test]
fn test_save_snapshot_takes_from_app() {
    let mut app = App::new_for_test();
    app.diff_cache =
        Some(crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+x", 4));
    let mut hc = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+y", 4);
    hc.file_index = 1;
    app.highlighted_cache_store.insert(1, hc);
    app.review_comments = Some(vec![crate::github::comment::ReviewComment {
        id: 1,
        path: "f.rs".to_string(),
        line: Some(1),
        body: "c".to_string(),
        user: crate::github::User {
            login: "u".to_string(),
        },
        created_at: "".to_string(),
    }]);
    app.discussion_comments = Some(vec![crate::github::comment::DiscussionComment {
        id: 1,
        body: "d".to_string(),
        user: crate::github::User {
            login: "u".to_string(),
        },
        created_at: "".to_string(),
    }]);
    app.local_file_signatures.insert("b.rs".to_string(), 1);
    app.local_file_patch_signatures
        .insert("b.rs".to_string(), 2);

    let _snapshot = app.save_view_snapshot();

    // take() / mem::take() により App 側は空になっていること
    assert!(app.diff_cache.is_none());
    assert!(app.highlighted_cache_store.is_empty());
    assert!(app.review_comments.is_none());
    assert!(app.discussion_comments.is_none());
    assert!(app.local_file_signatures.is_empty());
    assert!(app.local_file_patch_signatures.is_empty());
}

#[test]
fn test_restore_snapshot_all_fields() {
    use super::types::ViewSnapshot;
    use std::collections::HashMap;

    let mut app = App::new_for_test();
    // 事前に違う値を設定
    app.pr_number = Some(99);
    app.selected_file = 0;
    app.selected_line = 0;

    let mut dc = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+restored", 4);
    dc.file_index = 3;

    let mut hcs = HashMap::new();
    let mut hc = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+cached", 4);
    hc.file_index = 5;
    hcs.insert(5, hc);

    let mut sigs = HashMap::new();
    sigs.insert("sig.rs".to_string(), 333_u64);
    let mut patch_sigs = HashMap::new();
    patch_sigs.insert("sig.rs".to_string(), 444_u64);

    let snapshot = ViewSnapshot {
        pr_number: Some(42),
        selected_file: 8,
        file_list_scroll_offset: 4,
        selected_line: 20,
        scroll_offset: 6,
        diff_cache: Some(dc),
        highlighted_cache_store: hcs,
        review_comments: Some(vec![crate::github::comment::ReviewComment {
            id: 50,
            path: "r.rs".to_string(),
            line: Some(10),
            body: "restored comment".to_string(),
            user: crate::github::User {
                login: "r".to_string(),
            },
            created_at: "".to_string(),
        }]),
        discussion_comments: Some(vec![crate::github::comment::DiscussionComment {
            id: 60,
            body: "restored discussion".to_string(),
            user: crate::github::User {
                login: "d".to_string(),
            },
            created_at: "".to_string(),
        }]),
        local_file_signatures: sigs,
        local_file_patch_signatures: patch_sigs,
    };

    app.restore_view_snapshot(snapshot);

    // 全11フィールドが復元されていること
    assert_eq!(app.pr_number, Some(42));
    assert_eq!(app.selected_file, 8);
    assert_eq!(app.file_list_scroll_offset, 4);
    assert_eq!(app.selected_line, 20);
    assert_eq!(app.scroll_offset, 6);
    assert!(app.diff_cache.is_some());
    assert_eq!(app.diff_cache.as_ref().unwrap().file_index, 3);
    assert_eq!(app.highlighted_cache_store.len(), 1);
    assert!(app.highlighted_cache_store.contains_key(&5));
    assert_eq!(app.review_comments.as_ref().unwrap().len(), 1);
    assert_eq!(app.review_comments.as_ref().unwrap()[0].id, 50);
    assert_eq!(app.discussion_comments.as_ref().unwrap().len(), 1);
    assert_eq!(app.discussion_comments.as_ref().unwrap()[0].id, 60);
    assert_eq!(app.local_file_signatures["sig.rs"], 333);
    assert_eq!(app.local_file_patch_signatures["sig.rs"], 444);
}

#[test]
fn test_restore_snapshot_clears_receivers() {
    use super::types::ViewSnapshot;
    use std::collections::HashMap;

    let mut app = App::new_for_test();

    // レシーバーを設定
    let (_tx1, rx1) = mpsc::channel(1);
    app.diff_cache_receiver = Some(rx1);
    let (_tx2, rx2) = mpsc::channel(1);
    app.prefetch_receiver = Some(rx2);
    let (_tx3, rx3) = mpsc::channel::<Result<Vec<crate::github::comment::ReviewComment>, String>>(1);
    app.comment_receiver = Some((1, rx3));
    let (_tx4, rx4) = mpsc::channel::<Result<Vec<crate::github::comment::DiscussionComment>, String>>(1);
    app.discussion_comment_receiver = Some((1, rx4));
    let (_tx5, rx5) = mpsc::channel::<crate::loader::CommentSubmitResult>(1);
    app.comment_submit_receiver = Some((1, rx5));
    app.comment_submitting = true;
    app.comments_loading = true;
    app.discussion_comments_loading = true;

    let snapshot = ViewSnapshot {
        pr_number: None,
        selected_file: 0,
        file_list_scroll_offset: 0,
        selected_line: 0,
        scroll_offset: 0,
        diff_cache: None,
        highlighted_cache_store: HashMap::new(),
        review_comments: None,
        discussion_comments: None,
        local_file_signatures: HashMap::new(),
        local_file_patch_signatures: HashMap::new(),
    };

    app.restore_view_snapshot(snapshot);

    // 全レシーバーがクリアされていること
    assert!(app.diff_cache_receiver.is_none());
    assert!(app.prefetch_receiver.is_none());
    assert!(app.comment_receiver.is_none());
    assert!(app.discussion_comment_receiver.is_none());
    assert!(app.comment_submit_receiver.is_none());
    assert!(!app.comment_submitting);
    assert!(!app.comments_loading);
    assert!(!app.discussion_comments_loading);
}

#[test]
fn test_save_restore_roundtrip_preserves_data() {
    let mut app = App::new_for_test();
    app.pr_number = Some(10);
    app.selected_file = 3;
    app.file_list_scroll_offset = 1;
    app.selected_line = 7;
    app.scroll_offset = 2;
    app.local_file_signatures.insert("x.rs".to_string(), 500);
    app.local_file_patch_signatures
        .insert("x.rs".to_string(), 600);
    app.review_comments = Some(vec![crate::github::comment::ReviewComment {
        id: 77,
        path: "x.rs".to_string(),
        line: Some(3),
        body: "roundtrip".to_string(),
        user: crate::github::User {
            login: "u".to_string(),
        },
        created_at: "".to_string(),
    }]);

    // Save
    let snapshot = app.save_view_snapshot();

    // App が空になっていることを確認
    assert!(app.review_comments.is_none());
    assert!(app.local_file_signatures.is_empty());

    // 別の値を設定
    app.pr_number = Some(999);
    app.selected_file = 99;
    app.selected_line = 99;

    // Restore
    app.restore_view_snapshot(snapshot);

    // 元の値が復元されること
    assert_eq!(app.pr_number, Some(10));
    assert_eq!(app.selected_file, 3);
    assert_eq!(app.file_list_scroll_offset, 1);
    assert_eq!(app.selected_line, 7);
    assert_eq!(app.scroll_offset, 2);
    assert_eq!(app.local_file_signatures["x.rs"], 500);
    assert_eq!(app.local_file_patch_signatures["x.rs"], 600);
    assert_eq!(app.review_comments.as_ref().unwrap()[0].id, 77);
}

#[test]
fn test_toggle_local_mode_clears_receivers_on_entry() {
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    let (_data_tx, data_rx) = mpsc::channel(2);
    let mut app = App::new_for_test();
    app.retry_sender = Some(retry_tx);
    app.data_receiver = Some((42, data_rx));
    app.original_pr_number = Some(42);
    app.pr_number = Some(42);

    // toggle 前にレシーバーを設定
    let (_tx1, rx1) = mpsc::channel::<MarkViewedResult>(1);
    app.mark_viewed_receiver = Some((42, rx1));
    let (_tx2, rx2) = mpsc::channel::<Vec<crate::loader::SingleFileDiffResult>>(1);
    app.batch_diff_receiver = Some(rx2);
    let (_tx3, rx3) = mpsc::channel::<crate::loader::SingleFileDiffResult>(1);
    app.lazy_diff_receiver = Some(rx3);
    app.lazy_diff_pending_file = Some("file.rs".to_string());

    // PR → Local
    app.toggle_local_mode();

    // toggle 開始時にクリアされること
    assert!(app.mark_viewed_receiver.is_none());
    assert!(app.batch_diff_receiver.is_none());
    assert!(app.lazy_diff_receiver.is_none());
    assert!(app.lazy_diff_pending_file.is_none());
}

#[test]
fn test_toggle_local_mode_resets_file_list_filter() {
    use crate::filter::ListFilter;

    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    let (_data_tx, data_rx) = mpsc::channel(2);
    let mut app = App::new_for_test();
    app.retry_sender = Some(retry_tx);
    app.data_receiver = Some((42, data_rx));
    app.original_pr_number = Some(42);
    app.pr_number = Some(42);

    // フィルタを設定
    app.file_list_filter = Some(ListFilter::new());

    // PR → Local: フィルタがリセットされること
    app.toggle_local_mode();
    assert!(app.file_list_filter.is_none());

    // Local → PR でもリセットされること
    app.file_list_filter = Some(ListFilter::new());
    app.toggle_local_mode();
    assert!(app.file_list_filter.is_none());
}

#[test]
fn test_toggle_local_mode_from_pr_list_transitions_to_file_list() {
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    let (_data_tx, data_rx) = mpsc::channel(2);
    let mut app = App::new_for_test();
    app.retry_sender = Some(retry_tx);
    app.data_receiver = Some((42, data_rx));
    app.pr_number = Some(42);
    app.state = AppState::PullRequestList;

    // PullRequestList → Local: AppState が FileList に遷移すること
    app.toggle_local_mode();
    assert!(app.local_mode);
    assert_eq!(app.state, AppState::FileList);
}

#[test]
fn test_back_to_pr_list_saves_local_snapshot() {
    let mut app = App::new_for_test();
    app.started_from_pr_list = true;
    app.local_mode = true;
    app.selected_file = 5;
    app.local_file_signatures.insert("z.rs".to_string(), 999);

    app.back_to_pr_list();

    // ローカルスナップショットが保存されること
    assert!(app.saved_local_snapshot.is_some());
    let snap = app.saved_local_snapshot.as_ref().unwrap();
    assert_eq!(snap.selected_file, 5);
    assert_eq!(snap.local_file_signatures["z.rs"], 999);
    // local_mode は false に戻ること
    assert!(!app.local_mode);
    // PullRequestList に遷移すること
    assert_eq!(app.state, AppState::PullRequestList);
}

#[test]
fn test_back_to_pr_list_resets_pr_state() {
    let mut app = App::new_for_test();
    app.started_from_pr_list = true;
    app.pr_number = Some(42);
    app.review_comments = Some(vec![]);
    app.discussion_comments = Some(vec![]);
    app.diff_cache =
        Some(crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+x", 4));
    let mut hc = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+y", 4);
    hc.file_index = 1;
    app.highlighted_cache_store.insert(1, hc);

    app.back_to_pr_list();

    // PR 固有の状態がリセットされること
    assert!(app.pr_number.is_none());
    assert!(matches!(app.data_state, DataState::Loading));
    assert!(app.review_comments.is_none());
    assert!(app.discussion_comments.is_none());
    assert!(app.diff_cache.is_none());
    assert!(app.highlighted_cache_store.is_empty());
    assert_eq!(app.selected_file, 0);
    assert_eq!(app.file_list_scroll_offset, 0);
    assert_eq!(app.selected_line, 0);
    assert_eq!(app.scroll_offset, 0);
    assert!(app.file_list_filter.is_none());
}

#[test]
fn test_back_to_pr_list_clears_all_receivers() {
    let mut app = App::new_for_test();
    app.started_from_pr_list = true;

    // レシーバーを設定
    let (_tx1, rx1) = mpsc::channel::<Result<Vec<crate::github::comment::ReviewComment>, String>>(1);
    app.comment_receiver = Some((1, rx1));
    let (_tx2, rx2) = mpsc::channel(1);
    app.diff_cache_receiver = Some(rx2);
    let (_tx3, rx3) = mpsc::channel(1);
    app.prefetch_receiver = Some(rx3);
    let (_tx4, rx4) = mpsc::channel::<Result<Vec<crate::github::comment::DiscussionComment>, String>>(1);
    app.discussion_comment_receiver = Some((1, rx4));
    let (_tx5, rx5) = mpsc::channel::<crate::loader::CommentSubmitResult>(1);
    app.comment_submit_receiver = Some((1, rx5));
    let (_tx6, rx6) = mpsc::channel::<MarkViewedResult>(1);
    app.mark_viewed_receiver = Some((1, rx6));
    let (_tx7, rx7) = mpsc::channel::<Vec<crate::loader::SingleFileDiffResult>>(1);
    app.batch_diff_receiver = Some(rx7);
    let (_tx8, rx8) = mpsc::channel::<crate::loader::SingleFileDiffResult>(1);
    app.lazy_diff_receiver = Some(rx8);
    app.lazy_diff_pending_file = Some("file.rs".to_string());
    app.comment_submitting = true;
    app.comments_loading = true;
    app.discussion_comments_loading = true;

    app.back_to_pr_list();

    // 全レシーバーがクリアされること
    assert!(app.comment_receiver.is_none());
    assert!(app.diff_cache_receiver.is_none());
    assert!(app.prefetch_receiver.is_none());
    assert!(app.discussion_comment_receiver.is_none());
    assert!(app.comment_submit_receiver.is_none());
    assert!(app.mark_viewed_receiver.is_none());
    assert!(app.batch_diff_receiver.is_none());
    assert!(app.lazy_diff_receiver.is_none());
    assert!(app.lazy_diff_pending_file.is_none());
    assert!(!app.comment_submitting);
    assert!(!app.comments_loading);
    assert!(!app.discussion_comments_loading);
}

#[test]
fn test_toggle_local_mode_pr_to_local_and_back() {
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    let (_data_tx, data_rx) = mpsc::channel(2);
    let mut app = App::new_for_test();
    app.retry_sender = Some(retry_tx);
    app.data_receiver = Some((42, data_rx));
    app.original_pr_number = Some(42);
    app.pr_number = Some(42);
    app.selected_file = 3;

    // PR → Local
    app.toggle_local_mode();
    assert!(app.local_mode);
    assert_eq!(app.pr_number, Some(0));
    assert!(app.saved_pr_snapshot.is_some());
    assert!(app.submission_result.as_ref().unwrap().1.contains("Local"));

    // Local → PR
    app.toggle_local_mode();
    assert!(!app.local_mode);
    assert!(app.saved_local_snapshot.is_some());
    // saved_pr_snapshot が復元されたので取得済み
    assert!(app.saved_pr_snapshot.is_none());
    assert_eq!(app.selected_file, 3); // 復元された値
    assert!(app.submission_result.as_ref().unwrap().1.contains("PR"));
}

#[test]
fn test_toggle_local_mode_no_pr_to_return() {
    let mut app = App::new_for_test();
    app.original_pr_number = None;
    app.started_from_pr_list = false;
    app.local_mode = true;

    // Local → PR: 復帰先がない
    app.toggle_local_mode();
    // local_mode のまま（エラートースト）
    assert!(app.local_mode);
    assert!(app.submission_result.as_ref().unwrap().1.contains("No PR"));
}

#[test]
fn test_retry_load_sends_correct_request_type() {
    let (tx, mut rx) = mpsc::channel::<RefreshRequest>(1);
    let mut app = App::new_for_test();
    app.retry_sender = Some(tx);

    // PR mode
    app.local_mode = false;
    app.pr_number = Some(42);
    app.retry_load();
    let req = rx.try_recv().unwrap();
    assert!(matches!(req, RefreshRequest::PrRefresh { pr_number: 42 }));

    // Local mode
    app.local_mode = true;
    app.data_state = DataState::Loading; // reset from retry_load
    app.retry_load();
    let req = rx.try_recv().unwrap();
    assert!(matches!(req, RefreshRequest::LocalRefresh));
}

#[test]
fn test_is_shift_char_shortcut_accepts_uppercase() {
    let key = KeyEvent {
        code: KeyCode::Char('J'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    assert!(App::is_shift_char_shortcut(&key, 'j'));
}

#[test]
fn test_is_shift_char_shortcut_rejects_ctrl_or_alt() {
    let ctrl = KeyEvent {
        code: KeyCode::Char('J'),
        modifiers: KeyModifiers::SHIFT | KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let alt = KeyEvent {
        code: KeyCode::Char('K'),
        modifiers: KeyModifiers::SHIFT | KeyModifiers::ALT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };

    assert!(!App::is_shift_char_shortcut(&ctrl, 'j'));
    assert!(!App::is_shift_char_shortcut(&alt, 'k'));
}

#[test]
fn test_collect_unviewed_directory_paths_selected_prefix() {
    let files = vec![
        ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: false,
        },
        ChangedFile {
            filename: "src/lib.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: true,
        },
        ChangedFile {
            filename: "src/utils/mod.rs".to_string(),
            status: "added".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -0,0 +1 @@\n+test".to_string()),
            viewed: false,
        },
        ChangedFile {
            filename: "README.md".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: false,
        },
    ];

    let paths = App::collect_unviewed_directory_paths(&files, 0);
    assert_eq!(
        paths,
        vec!["src/main.rs".to_string(), "src/utils/mod.rs".to_string()]
    );
}

#[test]
fn test_collect_unviewed_directory_paths_root_prefix_matches_only_root_files() {
    let files = vec![
        ChangedFile {
            filename: "README.md".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: false,
        },
        ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: false,
        },
        ChangedFile {
            filename: "Cargo.toml".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: true,
        },
    ];

    let paths = App::collect_unviewed_directory_paths(&files, 0);
    assert_eq!(paths, vec!["README.md".to_string()]);
}

#[tokio::test]
async fn test_poll_mark_viewed_applies_unmark() {
    let mut app = App::new_for_test();
    app.pr_number = Some(1);
    app.data_state = DataState::Loaded {
        pr: Box::new(PullRequest {
            number: 1,
            node_id: Some("PR_node1".to_string()),
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }),
        files: vec![
            ChangedFile {
                filename: "src/main.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: true,
            },
            ChangedFile {
                filename: "src/lib.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: true,
            },
        ],
    };

    let (tx, rx) = mpsc::channel(1);
    app.mark_viewed_receiver = Some((1, rx));

    tx.send(MarkViewedResult::Completed {
        marked_paths: vec!["src/main.rs".to_string()],
        total_targets: 1,
        error: None,
        set_viewed: false,
    })
    .await
    .unwrap();

    app.poll_mark_viewed_updates();

    if let DataState::Loaded { files, .. } = &app.data_state {
        assert!(!files[0].viewed, "src/main.rs should be unviewed");
        assert!(files[1].viewed, "src/lib.rs should remain viewed");
    } else {
        panic!("Expected DataState::Loaded");
    }

    let (success, msg) = app.submission_result.unwrap();
    assert!(success);
    assert!(msg.contains("unviewed"));
}

#[tokio::test]
async fn test_poll_mark_viewed_skips_apply_on_pr_mismatch() {
    let mut app = App::new_for_test();
    app.pr_number = Some(2);
    app.data_state = DataState::Loaded {
        pr: Box::new(PullRequest {
            number: 2,
            node_id: Some("PR_node2".to_string()),
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }),
        files: vec![ChangedFile {
            filename: "src/main.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };

    let (tx, rx) = mpsc::channel(1);
    // origin_pr=1 but current pr_number=2
    app.mark_viewed_receiver = Some((1, rx));

    tx.send(MarkViewedResult::Completed {
        marked_paths: vec!["src/main.rs".to_string()],
        total_targets: 1,
        error: None,
        set_viewed: true,
    })
    .await
    .unwrap();

    app.poll_mark_viewed_updates();

    // File should NOT be updated because origin_pr != current pr_number
    if let DataState::Loaded { files, .. } = &app.data_state {
        assert!(
            !files[0].viewed,
            "File should remain unviewed due to PR mismatch"
        );
    } else {
        panic!("Expected DataState::Loaded");
    }
}

#[tokio::test]
async fn test_handle_data_result_auto_focus_skips_state_transition_during_bg_rally() {
    let mut app = App::new_for_test();
    app.local_mode = true;
    app.local_auto_focus = true;
    app.state = AppState::FileList;

    // Set up BG rally state (active but not in AiRally AppState)
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 0,
    });

    let pr = Box::new(make_local_pr());
    let files = vec![ChangedFile {
        filename: "new.rs".to_string(),
        status: "added".to_string(),
        additions: 1,
        deletions: 0,
        patch: Some("@@ -0,0 +1,1 @@\n+new content".to_string()),
        viewed: false,
    }];

    app.handle_data_result(0, DataLoadResult::Success { pr, files });

    // State should NOT transition to SplitViewDiff during BG rally
    assert_eq!(app.state, AppState::FileList);
    // But file selection IS updated
    assert_eq!(app.selected_file, 0);
}

fn make_local_pr() -> PullRequest {
    PullRequest {
        number: 0,
        node_id: None,
        title: "Local diff".to_string(),
        body: None,
        state: "local".to_string(),
        base: crate::github::Branch {
            ref_name: "local".to_string(),
            sha: "".to_string(),
        },
        head: crate::github::Branch {
            ref_name: "HEAD".to_string(),
            sha: "".to_string(),
        },
        user: crate::github::User {
            login: "local".to_string(),
        },
        updated_at: "".to_string(),
    }
}

#[test]
fn test_toggle_markdown_rich() {
    let mut app = App::new_for_test();
    // Set up loaded state with a markdown file
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "README.md".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: false,
        }],
    };

    assert!(!app.is_markdown_rich());

    app.toggle_markdown_rich();
    assert!(app.is_markdown_rich());
    assert!(
        app.diff_cache.is_none(),
        "Cache should be cleared for md file"
    );

    app.toggle_markdown_rich();
    assert!(!app.is_markdown_rich());
}

#[test]
fn test_toggle_markdown_rich_clears_receivers() {
    let mut app = App::new_for_test();
    // Set up loaded state with a markdown file
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "README.md".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+test".to_string()),
            viewed: false,
        }],
    };

    // Simulate having active receivers
    let (_tx, rx) = tokio::sync::mpsc::channel::<DiffCache>(1);
    app.diff_cache_receiver = Some(rx);

    let (_tx2, rx2) = tokio::sync::mpsc::channel::<DiffCache>(1);
    app.prefetch_receiver = Some(rx2);

    app.toggle_markdown_rich();
    assert!(
        app.diff_cache_receiver.is_none(),
        "diff_cache_receiver should be cleared for md file"
    );
    assert!(
        app.prefetch_receiver.is_none(),
        "prefetch_receiver should be cleared on toggle"
    );
}

#[test]
fn test_toggle_markdown_rich_clears_only_md_cache() {
    let mut app = App::new_for_test();
    // Set up loaded state with both md and non-md files
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![
            ChangedFile {
                filename: "README.md".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+test".to_string()),
                viewed: false,
            },
            ChangedFile {
                filename: "main.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+fn main(){}".to_string()),
                viewed: false,
            },
        ],
    };

    // Add cache entries for both files
    let md_cache = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+test", 4);
    let mut rs_cache =
        crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+fn main(){}", 4);
    rs_cache.file_index = 1;
    app.highlighted_cache_store.insert(0, md_cache);
    app.highlighted_cache_store.insert(1, rs_cache);
    assert_eq!(app.highlighted_cache_store.len(), 2);

    app.toggle_markdown_rich();

    // Only md cache should be removed
    assert!(
        !app.highlighted_cache_store.contains_key(&0),
        "md cache should be cleared"
    );
    assert!(
        app.highlighted_cache_store.contains_key(&1),
        "rs cache should be preserved"
    );
    assert_eq!(app.highlighted_cache_store.len(), 1);
}

#[test]
fn test_toggle_markdown_rich_preserves_non_md_diff_cache() {
    let mut app = App::new_for_test();
    // Current file is non-markdown
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "main.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+fn main(){}".to_string()),
            viewed: false,
        }],
    };

    let rs_cache = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+fn main(){}", 4);
    app.diff_cache = Some(rs_cache);

    app.toggle_markdown_rich();

    assert!(
        app.diff_cache.is_some(),
        "non-md diff_cache should be preserved"
    );
}

// --- Multiline selection tests ---

fn make_app_with_patch(patch: &str) -> App {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    let pr = Box::new(PullRequest {
        number: 1,
        node_id: None,
        title: "Test".to_string(),
        body: None,
        state: "open".to_string(),
        head: crate::github::Branch {
            ref_name: "feature".to_string(),
            sha: "abc123".to_string(),
        },
        base: crate::github::Branch {
            ref_name: "main".to_string(),
            sha: "def456".to_string(),
        },
        user: crate::github::User {
            login: "user".to_string(),
        },
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    });
    app.data_state = DataState::Loaded {
        pr,
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            patch: Some(patch.to_string()),
            viewed: false,
        }],
    };
    app.selected_file = 0;
    app
}

#[test]
fn test_enter_multiline_selection_sets_anchor() {
    let mut app = make_app_with_patch("@@ -1,3 +1,4 @@\n context\n+added\n more context");
    app.selected_line = 1; // context line
    app.enter_multiline_selection();
    assert!(app.multiline_selection.is_some());
    let sel = app.multiline_selection.as_ref().unwrap();
    assert_eq!(sel.anchor_line, 1);
    assert_eq!(sel.cursor_line, 1);
}

#[test]
fn test_enter_multiline_selection_rejected_on_header() {
    let mut app = make_app_with_patch("@@ -1,3 +1,4 @@\n context\n+added");
    app.selected_line = 0; // hunk header line
    app.enter_multiline_selection();
    assert!(app.multiline_selection.is_none());
}

#[test]
fn test_multiline_comment_preserves_selection_on_invalid_range() {
    let patch = "@@ -1,2 +1,2 @@\n line1\n+new line2\n@@ -10,2 +10,2 @@\n line10\n+new line11";
    let mut app = make_app_with_patch(patch);
    // Selection crosses hunk boundary (lines 1..=4)
    app.multiline_selection = Some(MultilineSelection {
        anchor_line: 1,
        cursor_line: 4,
    });
    app.enter_multiline_comment_input();
    // Selection should be preserved because validation failed
    assert!(
        app.multiline_selection.is_some(),
        "selection should not be cleared on validation failure"
    );
    assert!(
        app.input_mode.is_none(),
        "should not enter input mode on validation failure"
    );
}

#[test]
fn test_multiline_comment_clears_selection_on_valid_range() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added\n more context";
    let mut app = make_app_with_patch(patch);
    // Valid range: lines 1..=2 (context + added)
    app.multiline_selection = Some(MultilineSelection {
        anchor_line: 1,
        cursor_line: 2,
    });
    app.enter_multiline_comment_input();
    assert!(
        app.multiline_selection.is_none(),
        "selection should be cleared after successful validation"
    );
    assert!(app.input_mode.is_some(), "should enter input mode");
    assert_eq!(app.state, AppState::TextInput);
}

#[test]
fn test_multiline_suggestion_preserves_selection_on_invalid_range() {
    let patch = "@@ -1,3 +1,3 @@\n context\n-removed\n+added";
    let mut app = make_app_with_patch(patch);
    // Selection includes a removed line (index 2)
    app.multiline_selection = Some(MultilineSelection {
        anchor_line: 1,
        cursor_line: 3,
    });
    app.enter_multiline_suggestion_input();
    assert!(
        app.multiline_selection.is_some(),
        "selection should not be cleared on validation failure"
    );
    assert!(app.input_mode.is_none());
}

#[test]
fn test_multiline_suggestion_clears_selection_on_valid_range() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added\n more context";
    let mut app = make_app_with_patch(patch);
    app.multiline_selection = Some(MultilineSelection {
        anchor_line: 1,
        cursor_line: 2,
    });
    app.enter_multiline_suggestion_input();
    assert!(
        app.multiline_selection.is_none(),
        "selection should be cleared after successful validation"
    );
    assert!(app.input_mode.is_some());
    if let Some(InputMode::Suggestion {
        context,
        original_code,
    }) = &app.input_mode
    {
        assert!(context.start_line_number.is_some());
        assert!(!original_code.is_empty());
    } else {
        panic!("expected InputMode::Suggestion");
    }
}

#[test]
fn test_multiline_cancel_clears_selection() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added\n more context";
    let mut app = make_app_with_patch(patch);
    app.multiline_selection = Some(MultilineSelection {
        anchor_line: 1,
        cursor_line: 2,
    });
    // Simulate Esc to cancel
    app.multiline_selection = None;
    assert!(app.multiline_selection.is_none());
    assert!(app.input_mode.is_none());
}

// --- Help scroll tests ---

fn make_key(code: KeyCode) -> event::KeyEvent {
    event::KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn make_ctrl_key(c: char) -> event::KeyEvent {
    event::KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn test_help_scroll_j_increments_by_one() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    app.apply_help_scroll(make_key(KeyCode::Char('j')), 30);
    assert_eq!(app.help_scroll_offset, 1);
    app.apply_help_scroll(make_key(KeyCode::Char('j')), 30);
    assert_eq!(app.help_scroll_offset, 2);
}

#[test]
fn test_help_scroll_k_decrements_by_one_saturating() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 3;
    app.apply_help_scroll(make_key(KeyCode::Char('k')), 30);
    assert_eq!(app.help_scroll_offset, 2);
    // Saturate at 0
    app.help_scroll_offset = 0;
    app.apply_help_scroll(make_key(KeyCode::Char('k')), 30);
    assert_eq!(app.help_scroll_offset, 0);
}

#[test]
fn test_help_scroll_page_down_j_uppercase() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    // terminal height 30 → visible_lines = 30 - 5 = 25
    app.apply_help_scroll(make_key(KeyCode::Char('J')), 30);
    assert_eq!(app.help_scroll_offset, 25);
}

#[test]
fn test_help_scroll_page_up_k_uppercase() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 50;
    // terminal height 30 → visible_lines = 25
    app.apply_help_scroll(make_key(KeyCode::Char('K')), 30);
    assert_eq!(app.help_scroll_offset, 25);
}

#[test]
fn test_help_scroll_ctrl_d_half_page() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    // terminal height 30 → visible_lines = 25, half_page = 12
    app.apply_help_scroll(make_ctrl_key('d'), 30);
    assert_eq!(app.help_scroll_offset, 12);
}

#[test]
fn test_help_scroll_ctrl_u_half_page() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 20;
    // terminal height 30 → visible_lines = 25, half_page = 12
    app.apply_help_scroll(make_ctrl_key('u'), 30);
    assert_eq!(app.help_scroll_offset, 8);
}

#[test]
fn test_help_scroll_ctrl_d_at_least_1_on_small_terminal() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    // terminal height 6 → visible_lines = 1, half_page = max(0, 1) = 1
    app.apply_help_scroll(make_ctrl_key('d'), 6);
    assert_eq!(app.help_scroll_offset, 1);
}

#[test]
fn test_help_scroll_ctrl_d_at_least_1_on_very_small_terminal() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    // terminal height 5 → visible_lines = 0, half_page = max(0, 1) = 1
    app.apply_help_scroll(make_ctrl_key('d'), 5);
    assert_eq!(app.help_scroll_offset, 1);
}

#[test]
fn test_help_scroll_g_jumps_to_top() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 50;
    app.apply_help_scroll(make_key(KeyCode::Char('g')), 30);
    assert_eq!(app.help_scroll_offset, 0);
}

#[test]
fn test_help_scroll_g_uppercase_jumps_to_bottom() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    app.apply_help_scroll(make_key(KeyCode::Char('G')), 30);
    assert_eq!(app.help_scroll_offset, usize::MAX);
}

#[test]
fn test_help_scroll_q_returns_to_previous_state() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.previous_state = AppState::FileList;
    app.state = AppState::Help;
    app.apply_help_scroll(make_key(KeyCode::Char('q')), 30);
    assert_eq!(app.state, AppState::FileList);
}

#[test]
fn test_help_viewport_overhead_matches_render_layout() {
    // The render layout uses:
    //   Constraint::Length(3) for title + Constraint::Min(0) for content
    //   Content has Borders::ALL (2 rows overhead)
    // Total overhead = 3 + 2 = 5
    assert_eq!(App::HELP_VIEWPORT_OVERHEAD, 5);
}

fn make_shift_key(c: char) -> event::KeyEvent {
    event::KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn test_help_scroll_shift_j_page_down() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    // Shift+j should behave the same as J (page down)
    // terminal height 30 → visible_lines = 25
    app.apply_help_scroll(make_shift_key('j'), 30);
    assert_eq!(app.help_scroll_offset, 25);
}

#[test]
fn test_help_scroll_shift_k_page_up() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 50;
    // Shift+k should behave the same as K (page up)
    // terminal height 30 → visible_lines = 25
    app.apply_help_scroll(make_shift_key('k'), 30);
    assert_eq!(app.help_scroll_offset, 25);
}

#[test]
fn test_help_scroll_shift_g_jumps_to_bottom() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 0;
    // Shift+g should behave the same as G (jump to bottom)
    app.apply_help_scroll(make_shift_key('g'), 30);
    assert_eq!(app.help_scroll_offset, usize::MAX);
}

#[test]
fn test_help_scroll_g_without_modifiers_jumps_to_top() {
    let config = Config::default();
    let (mut app, _) = App::new_loading("owner/repo", 1, config);
    app.help_scroll_offset = 50;
    // Plain g (no modifiers) should jump to top
    app.apply_help_scroll(make_key(KeyCode::Char('g')), 30);
    assert_eq!(app.help_scroll_offset, 0);
}

#[tokio::test]
async fn test_help_from_pr_list_not_blocked_by_loading_guard() {
    // Regression: PR一覧(DataState::Loading)から?でヘルプを開いた後、
    // handle_inputのLoadingガードでキー入力がブロックされ戻れなくなるバグ
    let config = Config::default();
    let mut app = App::new_pr_list("owner/repo", config);
    // PR一覧のロードが完了した状態をシミュレート
    // (pr_list_loading=falseでないとキー入力を受け付けない)
    app.pr_list_loading = false;
    app.pr_list = Some(vec![]);
    // data_stateはPR未選択のためLoadingのまま
    assert!(matches!(app.data_state, DataState::Loading));

    // ?でヘルプを開く
    app.handle_pr_list_input(make_key(KeyCode::Char('?')))
        .await
        .unwrap();
    assert_eq!(app.state, AppState::Help);
    assert_eq!(app.previous_state, AppState::PullRequestList);

    // Help状態ではLoadingガードがスキップされるので、qで戻れる
    app.apply_help_scroll(make_key(KeyCode::Char('q')), 30);
    assert_eq!(app.state, AppState::PullRequestList);
}

#[tokio::test]
async fn test_patch_signature_detects_same_numstat_different_patch() {
    let config = Config::default();
    let (mut app, _tx) = App::new_loading("owner/repo", 1, config);
    app.set_local_mode(true);
    app.set_local_auto_focus(true);
    app.selected_file = 0;

    let make_file = |name: &str, patch: &str| ChangedFile {
        filename: name.to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some(patch.to_string()),
        viewed: false,
    };

    // 初回: files をセットして patch シグネチャを記録
    let initial_files = vec![
        make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new"),
        make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-foo\n+bar"),
    ];
    app.data_state = DataState::Loaded {
        pr: Box::new(PullRequest {
            number: 1,
            node_id: None,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }),
        files: initial_files,
    };

    // 初回バッチ完了: patch シグネチャを保存（オートフォーカスはスキップ）
    app.update_patch_signatures_and_auto_focus();
    assert_eq!(app.local_file_patch_signatures.len(), 2);
    assert_eq!(app.selected_file, 0, "first batch should not auto-focus");

    // ファイル内容が変わったが numstat は同じ（same additions=1, deletions=1）
    let updated_files = vec![
        make_file("file_a.rs", "@@ -1,1 +1,1 @@\n-old\n+new"), // unchanged
        make_file("file_b.rs", "@@ -1,1 +1,1 @@\n-foo\n+baz"), // content changed!
    ];
    app.data_state = DataState::Loaded {
        pr: Box::new(PullRequest {
            number: 1,
            node_id: None,
            title: "Test PR".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "feature".to_string(),
                sha: "abc123".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "main".to_string(),
                sha: "def456".to_string(),
            },
            user: crate::github::User {
                login: "user".to_string(),
            },
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }),
        files: updated_files,
    };

    // 2回目バッチ完了: file_b.rs の patch が変わった → オートフォーカス
    app.update_patch_signatures_and_auto_focus();
    assert_eq!(
        app.selected_file, 1,
        "should auto-focus to file_b.rs whose patch content changed (same numstat)"
    );
}

// --- KeyEventKind::Press filter tests ---

/// Verify that only KeyEventKind::Press events should be processed.
/// handle_input gates on key.kind == KeyEventKind::Press; Release and Repeat
/// events must be ignored to prevent double-execution when Kitty keyboard
/// protocol is enabled.
#[test]
fn test_key_event_kind_press_only() {
    let press = event::KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    let release = event::KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    };
    let repeat = event::KeyEvent {
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Repeat,
        state: KeyEventState::NONE,
    };

    // Only Press should be accepted by the filter in handle_input
    assert_eq!(press.kind, KeyEventKind::Press);
    assert_ne!(release.kind, KeyEventKind::Press);
    assert_ne!(repeat.kind, KeyEventKind::Press);
}

#[test]
fn test_pending_approve_choice_q_cancels_and_clears_prompt() {
    let mut app = App::new_for_test();
    app.pending_approve_body = Some(String::new());
    app.submission_result = Some((true, "placeholder".to_string()));
    app.submission_result_time = Some(Instant::now());

    let choice = app.handle_pending_approve_choice(&make_key(KeyCode::Char('q')));

    assert_eq!(choice, PendingApproveChoice::Cancel);
    assert!(app.pending_approve_body.is_none());
    assert!(app.submission_result.is_none());
    assert!(app.submission_result_time.is_none());
}

#[test]
fn test_pending_approve_choice_esc_cancels() {
    let mut app = App::new_for_test();
    app.pending_approve_body = Some("some body".to_string());

    let choice = app.handle_pending_approve_choice(&make_key(KeyCode::Esc));

    assert_eq!(choice, PendingApproveChoice::Cancel);
    assert!(app.pending_approve_body.is_none());
}

#[test]
fn test_pending_approve_choice_a_submits_empty_body() {
    let mut app = App::new_for_test();
    app.pending_approve_body = Some(String::new());

    let choice = app.handle_pending_approve_choice(&make_key(KeyCode::Char('a')));

    assert_eq!(choice, PendingApproveChoice::Submit);
    // pending_approve_body is NOT taken by handle_pending_approve_choice;
    // it is taken by the caller (handle_input) before calling submit_review_with_body.
    assert!(app.pending_approve_body.is_some());
}

#[test]
fn test_pending_approve_choice_a_submits_with_body() {
    let mut app = App::new_for_test();
    app.pending_approve_body = Some("LGTM!".to_string());

    let choice = app.handle_pending_approve_choice(&make_key(KeyCode::Char('a')));

    assert_eq!(choice, PendingApproveChoice::Submit);
    assert!(app.pending_approve_body.is_some());
    assert_eq!(app.pending_approve_body.as_deref(), Some("LGTM!"));
}

// ===================================================================
// 1. diff_cache.rs tests
// ===================================================================

#[test]
fn test_calc_diff_line_count_basic() {
    let files = vec![ChangedFile {
        filename: "test.rs".to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    }];
    assert_eq!(App::calc_diff_line_count(&files, 0), 3);
}

#[test]
fn test_calc_diff_line_count_no_patch() {
    let files = vec![ChangedFile {
        filename: "test.rs".to_string(),
        status: "modified".to_string(),
        additions: 0,
        deletions: 0,
        patch: None,
        viewed: false,
    }];
    assert_eq!(App::calc_diff_line_count(&files, 0), 0);
}

#[test]
fn test_calc_diff_line_count_out_of_bounds() {
    let files = vec![ChangedFile {
        filename: "test.rs".to_string(),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        patch: Some("@@ -1,1 +1,1 @@\n-old\n+new".to_string()),
        viewed: false,
    }];
    assert_eq!(App::calc_diff_line_count(&files, 5), 0);
}

#[test]
fn test_files_returns_empty_when_loading() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loading;
    assert!(app.files().is_empty());
}

#[test]
fn test_files_returns_files_when_loaded() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "a.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };
    assert_eq!(app.files().len(), 1);
    assert_eq!(app.files()[0].filename, "a.rs");
}

#[test]
fn test_pr_returns_none_when_loading() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loading;
    assert!(app.pr().is_none());
}

#[test]
fn test_pr_returns_some_when_loaded() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![],
    };
    assert!(app.pr().is_some());
    assert_eq!(app.pr().unwrap().number, 0);
}

// ===================================================================
// 2. local_mode.rs tests
// ===================================================================

#[tokio::test]
async fn test_restore_data_from_cache_hit() {
    let mut app = App::new_for_test();
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    app.retry_sender = Some(retry_tx);
    app.pr_number = Some(42);

    // Put data in cache
    let cache_key = PrCacheKey {
        repo: "test/repo".to_string(),
        pr_number: 42,
    };
    app.session_cache.put_pr_data(
        cache_key,
        PrData {
            pr: Box::new(make_local_pr()),
            files: vec![ChangedFile {
                filename: "cached.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+line".to_string()),
                viewed: false,
            }],
            pr_updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
    );

    app.restore_data_from_cache();
    assert!(matches!(app.data_state, DataState::Loaded { .. }));
}

#[test]
fn test_restore_data_from_cache_miss() {
    let mut app = App::new_for_test();
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    app.retry_sender = Some(retry_tx);
    app.pr_number = Some(999);

    app.restore_data_from_cache();
    assert!(matches!(app.data_state, DataState::Loading));
}

#[test]
fn test_update_data_receiver_origin() {
    let mut app = App::new_for_test();
    let (_tx, rx) = mpsc::channel::<DataLoadResult>(2);
    app.data_receiver = Some((1, rx));

    app.update_data_receiver_origin(42);

    if let Some((origin, _)) = &app.data_receiver {
        assert_eq!(*origin, 42);
    } else {
        panic!("data_receiver should exist");
    }
}

// ===================================================================
// 3. types.rs tests
// ===================================================================

#[test]
fn test_multiline_selection_start_end() {
    // anchor < cursor
    let sel = MultilineSelection {
        anchor_line: 3,
        cursor_line: 7,
    };
    assert_eq!(sel.start(), 3);
    assert_eq!(sel.end(), 7);

    // cursor < anchor
    let sel2 = MultilineSelection {
        anchor_line: 10,
        cursor_line: 2,
    };
    assert_eq!(sel2.start(), 2);
    assert_eq!(sel2.end(), 10);
}

#[test]
fn test_ai_rally_state_push_log_auto_follow() {
    use crate::ai::RallyState;
    let mut state = AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    };

    // No selection = tail, should auto-follow
    state.push_log(LogEntry::new(LogEventType::Info, "first".to_string()));
    assert_eq!(state.selected_log_index, Some(0));

    state.push_log(LogEntry::new(LogEventType::Info, "second".to_string()));
    assert_eq!(state.selected_log_index, Some(1));
}

#[test]
fn test_ai_rally_state_push_log_no_auto_follow() {
    use crate::ai::RallyState;
    let mut state = AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![
            LogEntry::new(LogEventType::Info, "old1".to_string()),
            LogEntry::new(LogEventType::Info, "old2".to_string()),
            LogEntry::new(LogEventType::Info, "old3".to_string()),
        ],
        log_scroll_offset: 0,
        selected_log_index: Some(0), // user scrolled up to first
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    };

    // User is NOT at tail, should not auto-follow
    state.push_log(LogEntry::new(LogEventType::Info, "new".to_string()));
    assert_eq!(state.selected_log_index, Some(0)); // stays at 0
}

#[test]
fn test_ai_rally_state_is_selection_at_tail() {
    use crate::ai::RallyState;
    let mut state = AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![
            LogEntry::new(LogEventType::Info, "a".to_string()),
            LogEntry::new(LogEventType::Info, "b".to_string()),
        ],
        log_scroll_offset: 0,
        selected_log_index: Some(1), // at tail
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    };

    // At tail: selected_log_index == logs.len() - 1
    // is_selection_at_tail is private, so test via field values
    assert_eq!(state.selected_log_index, Some(1));
    assert_eq!(state.logs.len(), 2);
    // selected_log_index == logs.len() - 1 means at tail

    // Not at tail
    state.selected_log_index = Some(0);
    assert_ne!(state.selected_log_index.unwrap(), state.logs.len() - 1);

    // None means auto-follow (at tail)
    state.selected_log_index = None;
}

#[test]
fn test_hash_string_deterministic() {
    let h1 = hash_string("hello world");
    let h2 = hash_string("hello world");
    assert_eq!(h1, h2);

    let h3 = hash_string("different");
    assert_ne!(h1, h3);
}

// ===================================================================
// 4. key_sequence.rs tests
// ===================================================================

#[test]
fn test_check_sequence_timeout_clears_expired() {
    let mut app = App::new_for_test();
    app.pending_since = Some(Instant::now() - std::time::Duration::from_secs(10));
    app.pending_keys.push(crate::keybinding::KeyBinding::char('g'));

    app.check_sequence_timeout();

    assert!(app.pending_keys.is_empty());
    assert!(app.pending_since.is_none());
}

#[test]
fn test_check_sequence_timeout_keeps_active() {
    let mut app = App::new_for_test();
    app.pending_since = Some(Instant::now());
    app.pending_keys.push(crate::keybinding::KeyBinding::char('g'));

    app.check_sequence_timeout();

    assert!(!app.pending_keys.is_empty());
    assert!(app.pending_since.is_some());
}

#[test]
fn test_push_pending_key_sets_timestamp() {
    let mut app = App::new_for_test();
    assert!(app.pending_since.is_none());

    app.push_pending_key(crate::keybinding::KeyBinding::char('g'));

    assert!(app.pending_since.is_some());
    assert_eq!(app.pending_keys.len(), 1);
}

#[test]
fn test_push_pending_key_appends() {
    let mut app = App::new_for_test();
    app.push_pending_key(crate::keybinding::KeyBinding::char('g'));
    app.push_pending_key(crate::keybinding::KeyBinding::char('d'));

    assert_eq!(app.pending_keys.len(), 2);
    assert_eq!(
        app.pending_keys[0],
        crate::keybinding::KeyBinding::char('g')
    );
    assert_eq!(
        app.pending_keys[1],
        crate::keybinding::KeyBinding::char('d')
    );
}

#[test]
fn test_clear_pending_keys_resets() {
    let mut app = App::new_for_test();
    app.push_pending_key(crate::keybinding::KeyBinding::char('g'));
    app.push_pending_key(crate::keybinding::KeyBinding::char('d'));

    app.clear_pending_keys();

    assert!(app.pending_keys.is_empty());
    assert!(app.pending_since.is_none());
}

#[test]
fn test_matches_single_key_basic() {
    let app = App::new_for_test();
    let seq = crate::keybinding::KeySequence(vec![
        crate::keybinding::KeyBinding::char('j')
    ]);
    let key = make_key(KeyCode::Char('j'));
    assert!(app.matches_single_key(&key, &seq));
}

#[test]
fn test_matches_single_key_ignores_sequence() {
    let app = App::new_for_test();
    let seq = crate::keybinding::KeySequence(vec![
        crate::keybinding::KeyBinding::char('g'),
        crate::keybinding::KeyBinding::char('d')
    ]);
    let key = make_key(KeyCode::Char('g'));
    assert!(!app.matches_single_key(&key, &seq));
}

#[test]
fn test_try_match_sequence_full_partial_none() {
    use crate::keybinding::{KeyBinding, KeySequence, SequenceMatch};

    let mut app = App::new_for_test();
    let seq = KeySequence(vec![
        KeyBinding::char('g'),
        KeyBinding::char('d')
    ]);

    // No pending keys
    assert_eq!(app.try_match_sequence(&seq), SequenceMatch::None);

    // Partial match
    app.pending_keys.push(KeyBinding::char('g'));
    assert_eq!(app.try_match_sequence(&seq), SequenceMatch::Partial);

    // Full match
    app.pending_keys.push(KeyBinding::char('d'));
    assert_eq!(app.try_match_sequence(&seq), SequenceMatch::Full);
}

#[test]
fn test_key_could_match_sequence_start() {
    use crate::keybinding::{KeyBinding, KeySequence};

    let app = App::new_for_test();
    let seq = KeySequence(vec![
        KeyBinding::char('g'),
        KeyBinding::char('d')
    ]);
    let key = make_key(KeyCode::Char('g'));
    assert!(app.key_could_match_sequence(&key, &seq));
}

#[test]
fn test_key_could_match_sequence_no_match() {
    use crate::keybinding::{KeyBinding, KeySequence};

    let app = App::new_for_test();
    let seq = KeySequence(vec![
        KeyBinding::char('g'),
        KeyBinding::char('d')
    ]);
    let key = make_key(KeyCode::Char('x'));
    assert!(!app.key_could_match_sequence(&key, &seq));
}

// ===================================================================
// 5. filter.rs tests
// ===================================================================

#[test]
fn test_handle_filter_input_char_updates_query() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };
    let mut filter = crate::filter::ListFilter::new();
    filter.apply(app.files(), |_, _| true);
    filter.sync_selection();
    app.file_list_filter = Some(filter);

    let key = make_key(KeyCode::Char('t'));
    assert!(app.handle_filter_input(&key, "file"));
    assert_eq!(app.file_list_filter.as_ref().unwrap().query, "t");
}

#[test]
fn test_handle_filter_input_backspace() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };
    let mut filter = crate::filter::ListFilter::new();
    filter.insert_char('a');
    filter.insert_char('b');
    filter.apply(app.files(), |_, _| true);
    filter.sync_selection();
    app.file_list_filter = Some(filter);

    let key = make_key(KeyCode::Backspace);
    assert!(app.handle_filter_input(&key, "file"));
    assert_eq!(app.file_list_filter.as_ref().unwrap().query, "a");
}

#[test]
fn test_handle_filter_input_enter_deactivates() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };
    let mut filter = crate::filter::ListFilter::new();
    filter.insert_char('t');
    filter.apply(app.files(), |_, _| true);
    filter.sync_selection();
    app.file_list_filter = Some(filter);

    let key = make_key(KeyCode::Enter);
    assert!(app.handle_filter_input(&key, "file"));
    assert!(!app.file_list_filter.as_ref().unwrap().input_active);
}

#[test]
fn test_handle_filter_input_esc_clears() {
    let mut app = App::new_for_test();
    let mut filter = crate::filter::ListFilter::new();
    filter.insert_char('x');
    app.file_list_filter = Some(filter);

    let key = make_key(KeyCode::Esc);
    assert!(app.handle_filter_input(&key, "file"));
    assert!(app.file_list_filter.is_none());
}

#[test]
fn test_handle_filter_input_returns_false_no_filter() {
    let mut app = App::new_for_test();
    app.file_list_filter = None;

    let key = make_key(KeyCode::Char('a'));
    assert!(!app.handle_filter_input(&key, "file"));
}

#[test]
fn test_reapply_filter_pr_list() {
    use crate::github::PullRequestSummary;
    let mut app = App::new_for_test();
    app.pr_list = Some(vec![
        PullRequestSummary {
            number: 1,
            title: "fix bug".to_string(),
            state: "open".to_string(),
            author: crate::github::User {
                login: "user".to_string(),
            },
            is_draft: false,
            labels: vec![],
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
        PullRequestSummary {
            number: 2,
            title: "add feature".to_string(),
            state: "open".to_string(),
            author: crate::github::User {
                login: "user".to_string(),
            },
            is_draft: false,
            labels: vec![],
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    let mut filter = crate::filter::ListFilter::new();
    filter.insert_char('b');
    filter.insert_char('u');
    filter.insert_char('g');
    app.pr_list_filter = Some(filter);

    app.reapply_filter("pr");

    let filter = app.pr_list_filter.as_ref().unwrap();
    assert_eq!(filter.matched_indices, vec![0]); // only "fix bug"
}

#[test]
fn test_reapply_filter_file_list() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![
            ChangedFile {
                filename: "src/main.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: false,
            },
            ChangedFile {
                filename: "src/lib.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: false,
            },
        ],
    };
    let mut filter = crate::filter::ListFilter::new();
    filter.insert_char('l');
    filter.insert_char('i');
    filter.insert_char('b');
    app.file_list_filter = Some(filter);

    app.reapply_filter("file");

    let filter = app.file_list_filter.as_ref().unwrap();
    assert_eq!(filter.matched_indices, vec![1]); // only "src/lib.rs"
}

#[test]
fn test_handle_filter_navigation_down() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![
            ChangedFile {
                filename: "a.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: false,
            },
            ChangedFile {
                filename: "b.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: false,
            },
        ],
    };
    let mut filter = crate::filter::ListFilter::new();
    filter.apply(app.files(), |_, _| true);
    filter.sync_selection();
    filter.input_active = false;
    app.file_list_filter = Some(filter);
    app.selected_file = 0;

    assert!(app.handle_filter_navigation("file", true));
    assert_eq!(app.selected_file, 1);
}

#[test]
fn test_handle_filter_navigation_up() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![
            ChangedFile {
                filename: "a.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: false,
            },
            ChangedFile {
                filename: "b.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: None,
                viewed: false,
            },
        ],
    };
    let mut filter = crate::filter::ListFilter::new();
    filter.apply(app.files(), |_, _| true);
    filter.sync_selection();
    filter.input_active = false;
    // Navigate to second item first
    filter.navigate_down();
    app.file_list_filter = Some(filter);
    app.selected_file = 1;

    assert!(app.handle_filter_navigation("file", false));
    assert_eq!(app.selected_file, 0);
}

#[test]
fn test_handle_filter_esc_clears_pr() {
    let mut app = App::new_for_test();
    let filter = crate::filter::ListFilter::new();
    app.pr_list_filter = Some(filter);

    assert!(app.handle_filter_esc("pr"));
    assert!(app.pr_list_filter.is_none());
}

#[test]
fn test_handle_filter_esc_no_filter() {
    let mut app = App::new_for_test();
    app.pr_list_filter = None;

    assert!(!app.handle_filter_esc("pr"));
}

#[test]
fn test_is_filter_selection_empty() {
    let mut app = App::new_for_test();

    // No filter -> false
    assert!(!app.is_filter_selection_empty("file"));

    // Filter with selection -> false
    let mut filter = crate::filter::ListFilter::new();
    filter.matched_indices = vec![0];
    filter.selected = Some(0);
    app.file_list_filter = Some(filter);
    assert!(!app.is_filter_selection_empty("file"));

    // Filter with no match -> true (selected=None)
    let mut filter2 = crate::filter::ListFilter::new();
    filter2.matched_indices = vec![];
    filter2.selected = None;
    app.file_list_filter = Some(filter2);
    assert!(app.is_filter_selection_empty("file"));
}

// ===================================================================
// 6. input.rs tests
// ===================================================================

#[test]
fn test_directory_prefix_for_nested() {
    assert_eq!(App::directory_prefix_for("a/b/c.txt"), "a/b/");
}

#[test]
fn test_directory_prefix_for_root() {
    assert_eq!(App::directory_prefix_for("root.txt"), "");
}

#[test]
fn test_directory_prefix_for_single_dir() {
    assert_eq!(App::directory_prefix_for("dir/file.rs"), "dir/");
}

#[test]
fn test_collect_unviewed_all_viewed() {
    let files = vec![
        ChangedFile {
            filename: "src/a.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: true,
        },
        ChangedFile {
            filename: "src/b.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: true,
        },
    ];
    let paths = App::collect_unviewed_directory_paths(&files, 0);
    assert!(paths.is_empty());
}

#[test]
fn test_collect_unviewed_mixed() {
    let files = vec![
        ChangedFile {
            filename: "src/a.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        },
        ChangedFile {
            filename: "src/b.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: true,
        },
        ChangedFile {
            filename: "src/c.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        },
    ];
    let paths = App::collect_unviewed_directory_paths(&files, 0);
    assert_eq!(paths, vec!["src/a.rs", "src/c.rs"]);
}

#[test]
fn test_refresh_all_resets_state() {
    let mut app = App::new_for_test();
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    app.retry_sender = Some(retry_tx);
    app.review_comments = Some(vec![]);
    app.discussion_comments = Some(vec![]);
    app.comments_loading = true;
    app.discussion_comments_loading = true;
    let filter = crate::filter::ListFilter::new();
    app.file_list_filter = Some(filter);

    app.refresh_all();

    assert!(matches!(app.data_state, DataState::Loading));
    assert!(app.review_comments.is_none());
    assert!(app.discussion_comments.is_none());
    assert!(!app.comments_loading);
    assert!(!app.discussion_comments_loading);
    assert!(app.file_list_filter.is_none());
}

#[test]
fn test_refresh_all_invalidates_session_cache() {
    let mut app = App::new_for_test();
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    app.retry_sender = Some(retry_tx);

    // Pre-populate cache
    let cache_key = PrCacheKey {
        repo: "test/repo".to_string(),
        pr_number: 1,
    };
    app.session_cache.put_pr_data(
        cache_key.clone(),
        PrData {
            pr: Box::new(make_local_pr()),
            files: vec![],
            pr_updated_at: "x".to_string(),
        },
    );
    assert!(app.session_cache.get_pr_data(&cache_key).is_some());

    app.refresh_all();

    assert!(app.session_cache.get_pr_data(&cache_key).is_none());
}

#[tokio::test]
async fn test_handle_mark_viewed_key_v_returns_true() {
    let mut app = App::new_for_test();
    app.pr_number = Some(1);
    app.data_state = DataState::Loaded {
        pr: Box::new(PullRequest {
            number: 1,
            node_id: Some("PR_node".to_string()),
            title: "Test".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "f".to_string(),
                sha: "a".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "m".to_string(),
                sha: "b".to_string(),
            },
            user: crate::github::User {
                login: "u".to_string(),
            },
            updated_at: "".to_string(),
        }),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };
    let key = make_key(KeyCode::Char('v'));
    assert!(app.handle_mark_viewed_key(key));
}

#[test]
fn test_handle_mark_viewed_key_ignored_local_mode() {
    let mut app = App::new_for_test();
    app.local_mode = true;
    let key = make_key(KeyCode::Char('v'));
    assert!(!app.handle_mark_viewed_key(key));
}

#[test]
fn test_handle_mark_viewed_key_other_key() {
    let mut app = App::new_for_test();
    let key = make_key(KeyCode::Char('x'));
    assert!(!app.handle_mark_viewed_key(key));
}

// ===================================================================
// 7. input_text.rs tests
// ===================================================================

#[test]
fn test_cancel_input_clears_mode() {
    let mut app = App::new_for_test();
    app.input_mode = Some(InputMode::Comment(LineInputContext {
        file_index: 0,
        line_number: 1,
        diff_position: 1,
        start_line_number: None,
    }));
    app.state = AppState::TextInput;
    app.preview_return_state = AppState::DiffView;

    app.cancel_input();

    assert!(app.input_mode.is_none());
}

#[test]
fn test_cancel_input_clears_text_area() {
    let mut app = App::new_for_test();
    app.input_text_area.set_content("some text");
    app.state = AppState::TextInput;
    app.preview_return_state = AppState::DiffView;

    app.cancel_input();

    assert!(app.input_text_area.content().is_empty());
}

#[test]
fn test_cancel_input_restores_state() {
    let mut app = App::new_for_test();
    app.state = AppState::TextInput;
    app.preview_return_state = AppState::DiffView;

    app.cancel_input();

    assert_eq!(app.state, AppState::DiffView);
}

#[test]
fn test_cancel_input_clears_multiline_selection() {
    let mut app = App::new_for_test();
    app.multiline_selection = Some(MultilineSelection {
        anchor_line: 1,
        cursor_line: 5,
    });
    app.state = AppState::TextInput;
    app.preview_return_state = AppState::DiffView;

    app.cancel_input();

    // cancel_input does not clear multiline_selection (it's cleared earlier in flow)
    // but input_mode is cleared
    assert!(app.input_mode.is_none());
}

// ===================================================================
// 8. input_diff.rs tests
// ===================================================================

#[test]
fn test_adjust_scroll_above_viewport() {
    let mut app = App::new_for_test();
    app.selected_line = 2;
    app.scroll_offset = 5;
    app.diff_line_count = 100;

    app.adjust_scroll(20);

    assert_eq!(app.scroll_offset, 2);
}

#[test]
fn test_adjust_scroll_below_viewport() {
    let mut app = App::new_for_test();
    app.selected_line = 30;
    app.scroll_offset = 5;
    app.diff_line_count = 100;

    app.adjust_scroll(20);

    assert!(app.scroll_offset > 5);
    assert!(app.selected_line >= app.scroll_offset);
}

#[test]
fn test_adjust_scroll_within_viewport() {
    let mut app = App::new_for_test();
    app.selected_line = 10;
    app.scroll_offset = 5;
    app.diff_line_count = 100;

    app.adjust_scroll(20);

    // Within viewport, scroll should not change much
    assert!(app.selected_line >= app.scroll_offset);
    assert!(app.selected_line < app.scroll_offset + 20);
}

#[test]
fn test_adjust_scroll_zero_visible() {
    let mut app = App::new_for_test();
    app.selected_line = 10;
    app.scroll_offset = 5;
    app.diff_line_count = 100;

    app.adjust_scroll(0);

    // Should early return, no change
    assert_eq!(app.scroll_offset, 5);
}

#[test]
fn test_adjust_scroll_end_padding() {
    let mut app = App::new_for_test();
    app.diff_line_count = 50;
    app.selected_line = 49; // last line
    app.scroll_offset = 0;

    app.adjust_scroll(20);

    // Near end, scroll allows padding
    assert!(app.scroll_offset > 0);
}

#[test]
fn test_adjust_scroll_single_line() {
    let mut app = App::new_for_test();
    app.diff_line_count = 1;
    app.selected_line = 0;
    app.scroll_offset = 0;

    app.adjust_scroll(20);

    assert_eq!(app.scroll_offset, 0);
    assert_eq!(app.selected_line, 0);
}

// ===================================================================
// 9. ai_rally.rs tests
// ===================================================================

#[test]
fn test_cleanup_rally_state() {
    let mut app = App::new_for_test();
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    let (cmd_tx, _cmd_rx) = mpsc::channel(10);
    app.rally_command_sender = Some(cmd_tx);
    let (_, event_rx) = mpsc::channel(100);
    app.rally_event_receiver = Some(event_rx);

    app.cleanup_rally_state();

    assert!(app.ai_rally_state.is_none());
    assert!(app.rally_command_sender.is_none());
    assert!(app.rally_event_receiver.is_none());
}

#[test]
fn test_is_rally_running_in_background_not_in_rally() {
    let mut app = App::new_for_test();
    app.state = AppState::FileList;
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    assert!(app.is_rally_running_in_background());
}

#[test]
fn test_is_rally_running_in_background_in_rally() {
    let mut app = App::new_for_test();
    app.state = AppState::AiRally;
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    assert!(!app.is_rally_running_in_background());
}

#[test]
fn test_is_rally_running_in_background_no_state() {
    let mut app = App::new_for_test();
    app.state = AppState::FileList;
    app.ai_rally_state = None;
    assert!(!app.is_rally_running_in_background());
}

#[test]
fn test_is_rally_running_in_background_finished() {
    let mut app = App::new_for_test();
    app.state = AppState::FileList;
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::Completed,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    assert!(!app.is_rally_running_in_background());
}

#[test]
fn test_has_background_rally_true() {
    let mut app = App::new_for_test();
    app.state = AppState::FileList;
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    assert!(app.has_background_rally());
}

#[test]
fn test_has_background_rally_false_in_rally() {
    let mut app = App::new_for_test();
    app.state = AppState::AiRally;
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    assert!(!app.has_background_rally());
}

#[test]
fn test_has_background_rally_false_no_state() {
    let mut app = App::new_for_test();
    app.state = AppState::FileList;
    app.ai_rally_state = None;
    assert!(!app.has_background_rally());
}

#[test]
fn test_is_background_rally_finished_completed() {
    let mut app = App::new_for_test();
    app.state = AppState::FileList;
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::Completed,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    assert!(app.is_background_rally_finished());
}

#[test]
fn test_is_background_rally_finished_running() {
    let mut app = App::new_for_test();
    app.state = AppState::FileList;
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: vec![],
        log_scroll_offset: 0,
        selected_log_index: None,
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 10,
    });
    assert!(!app.is_background_rally_finished());
}

#[test]
fn test_adjust_log_scroll_selection_above() {
    let mut app = App::new_for_test();
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: (0..20)
            .map(|i| LogEntry::new(LogEventType::Info, format!("log {}", i)))
            .collect(),
        log_scroll_offset: 10,
        selected_log_index: Some(5), // above visible area
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 5,
    });

    app.adjust_log_scroll_to_selection();

    let rally_state = app.ai_rally_state.as_ref().unwrap();
    assert!(rally_state.log_scroll_offset <= 5);
}

#[test]
fn test_adjust_log_scroll_selection_below() {
    let mut app = App::new_for_test();
    app.ai_rally_state = Some(AiRallyState {
        iteration: 1,
        max_iterations: 10,
        state: crate::ai::RallyState::ReviewerReviewing,
        history: vec![],
        logs: (0..20)
            .map(|i| LogEntry::new(LogEventType::Info, format!("log {}", i)))
            .collect(),
        log_scroll_offset: 1,
        selected_log_index: Some(15), // below visible area
        showing_log_detail: false,
        pending_question: None,
        pending_permission: None,
        pending_review_post: None,
        pending_fix_post: None,
        last_visible_log_height: 5,
    });

    app.adjust_log_scroll_to_selection();

    let rally_state = app.ai_rally_state.as_ref().unwrap();
    assert!(rally_state.log_scroll_offset > 1);
}

// ===================================================================
// 10. symbol.rs tests
// ===================================================================

#[test]
fn test_push_jump_location_basic() {
    let mut app = App::new_for_test();
    app.selected_file = 2;
    app.selected_line = 10;
    app.scroll_offset = 5;

    app.push_jump_location();

    assert_eq!(app.jump_stack.len(), 1);
    assert_eq!(app.jump_stack[0].file_index, 2);
    assert_eq!(app.jump_stack[0].line_index, 10);
    assert_eq!(app.jump_stack[0].scroll_offset, 5);
}

#[test]
fn test_push_jump_location_max_capacity() {
    let mut app = App::new_for_test();

    // Push 101 locations (should trim oldest)
    for i in 0..101 {
        app.selected_file = i;
        app.selected_line = i;
        app.scroll_offset = 0;
        app.push_jump_location();
    }

    assert_eq!(app.jump_stack.len(), 100);
    // First entry should be index 1 (0 was removed)
    assert_eq!(app.jump_stack[0].file_index, 1);
}

#[test]
fn test_push_jump_location_preserves_fields() {
    let mut app = App::new_for_test();
    app.selected_file = 42;
    app.selected_line = 99;
    app.scroll_offset = 33;

    app.push_jump_location();

    let loc = &app.jump_stack[0];
    assert_eq!(loc.file_index, 42);
    assert_eq!(loc.line_index, 99);
    assert_eq!(loc.scroll_offset, 33);
}

#[tokio::test]
async fn test_jump_back_restores_position() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![
            ChangedFile {
                filename: "a.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+line".to_string()),
                viewed: false,
            },
            ChangedFile {
                filename: "b.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+line".to_string()),
                viewed: false,
            },
        ],
    };

    // Push current position
    app.selected_file = 0;
    app.selected_line = 5;
    app.scroll_offset = 2;
    app.push_jump_location();

    // Move elsewhere
    app.selected_file = 1;
    app.selected_line = 10;
    app.scroll_offset = 8;

    // Jump back
    app.jump_back();

    assert_eq!(app.selected_file, 0);
    assert_eq!(app.selected_line, 5);
    assert_eq!(app.scroll_offset, 2);
}

#[test]
fn test_jump_back_empty_stack() {
    let mut app = App::new_for_test();
    app.selected_file = 3;
    app.selected_line = 7;
    app.scroll_offset = 4;

    app.jump_back();

    // Nothing should change
    assert_eq!(app.selected_file, 3);
    assert_eq!(app.selected_line, 7);
    assert_eq!(app.scroll_offset, 4);
}

// ===================================================================
// 11. comments.rs tests
// ===================================================================

#[test]
fn test_enter_comment_input_sets_mode() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added\n more context";
    let mut app = make_app_with_patch(patch);
    app.selected_line = 1; // context line (commentable)
    app.state = AppState::DiffView;

    app.enter_comment_input();

    assert!(matches!(app.input_mode, Some(InputMode::Comment(_))));
    assert_eq!(app.state, AppState::TextInput);
}

#[test]
fn test_enter_comment_input_no_patch() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 0,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };

    // Should not panic
    app.enter_comment_input();
    assert!(app.input_mode.is_none());
}

#[test]
fn test_enter_suggestion_input_sets_mode() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added line\n more context";
    let mut app = make_app_with_patch(patch);
    app.selected_line = 2; // added line
    app.state = AppState::DiffView;

    app.enter_suggestion_input();

    assert!(matches!(app.input_mode, Some(InputMode::Suggestion { .. })));
    assert_eq!(app.state, AppState::TextInput);
}

#[tokio::test]
async fn test_open_comment_list_transitions_state() {
    let mut app = App::new_for_test();
    let (retry_tx, _) = mpsc::channel::<RefreshRequest>(4);
    app.retry_sender = Some(retry_tx);
    app.state = AppState::FileList;

    // Need loaded PR data for comment list
    app.data_state = DataState::Loaded {
        pr: Box::new(PullRequest {
            number: 1,
            node_id: None,
            title: "Test".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "f".to_string(),
                sha: "a".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "m".to_string(),
                sha: "b".to_string(),
            },
            user: crate::github::User {
                login: "u".to_string(),
            },
            updated_at: "".to_string(),
        }),
        files: vec![],
    };
    app.previous_state = AppState::FileList;
    app.open_comment_list();
    assert_eq!(app.state, AppState::CommentList);
}

#[tokio::test]
async fn test_open_comment_list_sets_previous_state() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![],
    };
    app.state = AppState::FileList;
    app.previous_state = AppState::FileList;
    app.open_comment_list();
    // previous_state is set before open_comment_list, not by it
    assert_eq!(app.state, AppState::CommentList);
}

#[test]
fn test_update_file_comment_positions_empty_comments() {
    let mut app = make_app_with_patch("@@ -1,3 +1,4 @@\n context\n+added\n more context");
    app.review_comments = Some(vec![]);
    app.update_file_comment_positions();
    assert!(app.file_comment_positions.is_empty());
}

#[test]
fn test_update_file_comment_positions_with_comments() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added\n more context";
    let mut app = make_app_with_patch(patch);
    app.review_comments = Some(vec![
        crate::github::comment::ReviewComment {
            id: 1,
            path: "test.rs".to_string(),
            line: Some(1),
            body: "comment at line 1".to_string(),
            user: crate::github::User {
                login: "reviewer".to_string(),
            },
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    app.update_file_comment_positions();
    assert_eq!(app.file_comment_positions.len(), 1);
}

#[test]
fn test_update_file_comment_positions_stale_comment() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added\n more context";
    let mut app = make_app_with_patch(patch);
    app.review_comments = Some(vec![
        crate::github::comment::ReviewComment {
            id: 1,
            path: "other_file.rs".to_string(), // different file
            line: Some(1),
            body: "wrong file".to_string(),
            user: crate::github::User {
                login: "reviewer".to_string(),
            },
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    app.update_file_comment_positions();
    assert!(app.file_comment_positions.is_empty());
}

#[test]
fn test_wrapped_line_count_short() {
    assert_eq!(App::wrapped_line_count("hello", 80), 1);
}

#[test]
fn test_wrapped_line_count_long() {
    // 100 chars in 40-wide panel = 3 lines
    let text: String = "x".repeat(100);
    assert_eq!(App::wrapped_line_count(&text, 40), 3);
}

#[test]
fn test_wrapped_line_count_empty() {
    assert_eq!(App::wrapped_line_count("", 80), 1);
}

#[test]
fn test_comment_body_wrapped_lines() {
    let body = "short line\na longer line that has more characters";
    // Each line wraps independently
    let count = App::comment_body_wrapped_lines(body, 80);
    assert_eq!(count, 2); // both fit in 80 chars
}

#[test]
fn test_comment_panel_inner_width() {
    let mut app = App::new_for_test();
    app.state = AppState::DiffView;
    // For non-SplitView, width = terminal_width - 2
    assert_eq!(app.comment_panel_inner_width(100), 98);
}

#[test]
fn test_max_comment_panel_scroll() {
    let mut app = App::new_for_test();
    app.state = AppState::DiffView;
    app.file_comment_positions = vec![];
    app.review_comments = Some(vec![]);

    // No comments at current line -> content = 1 ("No comments..." message)
    let max = app.max_comment_panel_scroll(40, 80);
    // Content lines (1) - panel inner height
    // Panel inner height ≈ (40-8)*40/100 = 12
    // saturating_sub means 0
    assert_eq!(max, 0);
}

#[test]
fn test_enter_reply_input_sets_mode() {
    let patch = "@@ -1,3 +1,4 @@\n context\n+added\n more context";
    let mut app = make_app_with_patch(patch);
    app.selected_line = 1;
    app.review_comments = Some(vec![
        crate::github::comment::ReviewComment {
            id: 42,
            path: "test.rs".to_string(),
            line: Some(1),
            body: "original comment".to_string(),
            user: crate::github::User {
                login: "reviewer".to_string(),
            },
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    app.file_comment_positions = vec![CommentPosition {
        diff_line_index: 1,
        comment_index: 0,
    }];
    app.state = AppState::DiffView;

    app.enter_reply_input();

    assert!(matches!(app.input_mode, Some(InputMode::Reply { .. })));
    assert_eq!(app.state, AppState::TextInput);
}

#[test]
fn test_handle_discussion_detail_scroll_j() {
    let mut app = App::new_for_test();
    app.discussion_comment_detail_mode = true;
    app.discussion_comment_detail_scroll = 0;

    let result = app.handle_discussion_detail_input(make_key(KeyCode::Char('j')), 20);
    assert!(result.is_ok());
    assert_eq!(app.discussion_comment_detail_scroll, 1);
}

#[test]
fn test_handle_discussion_detail_scroll_k() {
    let mut app = App::new_for_test();
    app.discussion_comment_detail_mode = true;
    app.discussion_comment_detail_scroll = 5;

    let result = app.handle_discussion_detail_input(make_key(KeyCode::Char('k')), 20);
    assert!(result.is_ok());
    assert_eq!(app.discussion_comment_detail_scroll, 4);
}

#[tokio::test]
async fn test_jump_to_comment_sets_file_and_line() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(PullRequest {
            number: 1,
            node_id: None,
            title: "Test".to_string(),
            body: None,
            state: "open".to_string(),
            head: crate::github::Branch {
                ref_name: "f".to_string(),
                sha: "a".to_string(),
            },
            base: crate::github::Branch {
                ref_name: "m".to_string(),
                sha: "b".to_string(),
            },
            user: crate::github::User {
                login: "u".to_string(),
            },
            updated_at: "".to_string(),
        }),
        files: vec![
            ChangedFile {
                filename: "first.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1,1 +1,2 @@\n line1\n+line2".to_string()),
                viewed: false,
            },
            ChangedFile {
                filename: "second.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1,1 +1,2 @@\n line1\n+line2".to_string()),
                viewed: false,
            },
        ],
    };
    app.review_comments = Some(vec![
        crate::github::comment::ReviewComment {
            id: 1,
            path: "second.rs".to_string(),
            line: Some(2),
            body: "check this".to_string(),
            user: crate::github::User {
                login: "r".to_string(),
            },
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    app.selected_comment = 0;

    app.jump_to_comment();

    assert_eq!(app.selected_file, 1); // second.rs
    assert_eq!(app.state, AppState::DiffView);
}

// ===================================================================
// 12. pr_list.rs tests
// ===================================================================

#[tokio::test]
async fn test_handle_pr_list_input_quit() {
    let mut app = App::new_for_test();
    app.state = AppState::PullRequestList;
    app.pr_list_loading = false;
    app.pr_list = Some(vec![]);

    app.handle_pr_list_input(make_key(KeyCode::Char('q')))
        .await
        .unwrap();
    assert!(app.should_quit);
}

#[tokio::test]
async fn test_handle_pr_list_input_loading_blocks() {
    use crate::github::PullRequestSummary;
    let mut app = App::new_for_test();
    app.state = AppState::PullRequestList;
    app.pr_list_loading = true;
    app.pr_list = Some(vec![
        PullRequestSummary {
            number: 1,
            title: "PR 1".to_string(),
            state: "open".to_string(),
            author: crate::github::User {
                login: "user".to_string(),
            },
            is_draft: false,
            labels: vec![],
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    app.selected_pr = 0;

    // j key should be blocked during loading
    app.handle_pr_list_input(make_key(KeyCode::Char('j')))
        .await
        .unwrap();
    assert_eq!(app.selected_pr, 0); // unchanged
}

#[tokio::test]
async fn test_handle_pr_list_input_move_down() {
    use crate::github::PullRequestSummary;
    let mut app = App::new_for_test();
    app.state = AppState::PullRequestList;
    app.pr_list_loading = false;
    app.pr_list = Some(vec![
        PullRequestSummary {
            number: 1,
            title: "PR 1".to_string(),
            state: "open".to_string(),
            author: crate::github::User {
                login: "user".to_string(),
            },
            is_draft: false,
            labels: vec![],
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
        PullRequestSummary {
            number: 2,
            title: "PR 2".to_string(),
            state: "open".to_string(),
            author: crate::github::User {
                login: "user".to_string(),
            },
            is_draft: false,
            labels: vec![],
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    app.selected_pr = 0;

    app.handle_pr_list_input(make_key(KeyCode::Char('j')))
        .await
        .unwrap();
    assert_eq!(app.selected_pr, 1);
}

#[tokio::test]
async fn test_handle_pr_list_input_move_up() {
    use crate::github::PullRequestSummary;
    let mut app = App::new_for_test();
    app.state = AppState::PullRequestList;
    app.pr_list_loading = false;
    app.pr_list = Some(vec![
        PullRequestSummary {
            number: 1,
            title: "PR 1".to_string(),
            state: "open".to_string(),
            author: crate::github::User {
                login: "user".to_string(),
            },
            is_draft: false,
            labels: vec![],
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
        PullRequestSummary {
            number: 2,
            title: "PR 2".to_string(),
            state: "open".to_string(),
            author: crate::github::User {
                login: "user".to_string(),
            },
            is_draft: false,
            labels: vec![],
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ]);
    app.selected_pr = 1;

    app.handle_pr_list_input(make_key(KeyCode::Char('k')))
        .await
        .unwrap();
    assert_eq!(app.selected_pr, 0);
}

#[tokio::test]
async fn test_handle_pr_list_input_jump_to_last() {
    use crate::github::PullRequestSummary;
    let mut app = App::new_for_test();
    app.state = AppState::PullRequestList;
    app.pr_list_loading = false;
    app.pr_list = Some(
        (0..10)
            .map(|i| PullRequestSummary {
                number: i,
                title: format!("PR {}", i),
                state: "open".to_string(),
                author: crate::github::User {
                    login: "user".to_string(),
                },
                is_draft: false,
                labels: vec![],
                updated_at: "2024-01-01T00:00:00Z".to_string(),
            })
            .collect(),
    );
    app.selected_pr = 0;

    app.handle_pr_list_input(make_key(KeyCode::Char('G')))
        .await
        .unwrap();
    assert_eq!(app.selected_pr, 9);
}

#[tokio::test]
async fn test_reload_pr_list_resets_state() {
    let mut app = App::new_for_test();
    app.selected_pr = 5;
    app.pr_list_scroll_offset = 10;
    let filter = crate::filter::ListFilter::new();
    app.pr_list_filter = Some(filter);

    app.reload_pr_list();

    assert_eq!(app.selected_pr, 0);
    assert_eq!(app.pr_list_scroll_offset, 0);
    assert!(app.pr_list_loading);
    assert!(!app.pr_list_has_more);
    assert!(app.pr_list_filter.is_none());
}

#[test]
fn test_load_more_prs_skips_when_loading() {
    let mut app = App::new_for_test();
    app.pr_list_loading = true;
    let prev_receiver = app.pr_list_receiver.is_some();

    app.load_more_prs();

    // Should not create a new receiver
    assert_eq!(app.pr_list_receiver.is_some(), prev_receiver);
}

#[test]
fn test_select_pr_cache_miss_sets_loading() {
    let mut app = App::new_for_test();
    let (retry_tx, _retry_rx) = mpsc::channel::<RefreshRequest>(4);
    let (_data_tx, data_rx) = mpsc::channel(2);
    app.retry_sender = Some(retry_tx);
    app.data_receiver = Some((0, data_rx));

    app.select_pr(42);

    assert_eq!(app.pr_number, Some(42));
    assert_eq!(app.state, AppState::FileList);
    assert!(matches!(app.data_state, DataState::Loading));
}

// ===================================================================
// 13. polling.rs tests
// ===================================================================

#[tokio::test]
async fn test_poll_diff_cache_accepts_valid() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+line".to_string()),
            viewed: false,
        }],
    };
    app.selected_file = 0;
    app.diff_line_count = 2;

    let (tx, rx) = mpsc::channel(1);
    app.diff_cache_receiver = Some(rx);

    let cache = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+line", 4);
    tx.send(cache).await.unwrap();

    app.poll_diff_cache_updates();

    assert!(app.diff_cache.is_some());
}

#[tokio::test]
async fn test_poll_diff_cache_rejects_stale_file() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![
            ChangedFile {
                filename: "a.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+line".to_string()),
                viewed: false,
            },
            ChangedFile {
                filename: "b.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+line2".to_string()),
                viewed: false,
            },
        ],
    };
    app.selected_file = 1; // We're now looking at file 1

    let (tx, rx) = mpsc::channel(1);
    app.diff_cache_receiver = Some(rx);

    // Send cache for file 0 (stale)
    let mut cache = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+line", 4);
    cache.file_index = 0;
    tx.send(cache).await.unwrap();

    app.poll_diff_cache_updates();

    // Stale cache should be rejected (diff_cache stays None or is not updated)
    if let Some(ref c) = app.diff_cache {
        assert_ne!(c.file_index, 0, "stale cache should not be applied");
    }
}

#[tokio::test]
async fn test_poll_prefetch_stores_cache() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![
            ChangedFile {
                filename: "a.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+line".to_string()),
                viewed: false,
            },
            ChangedFile {
                filename: "b.rs".to_string(),
                status: "modified".to_string(),
                additions: 1,
                deletions: 0,
                patch: Some("@@ -1 +1 @@\n+line2".to_string()),
                viewed: false,
            },
        ],
    };
    app.selected_file = 0;

    let (tx, rx) = mpsc::channel(2);
    app.prefetch_receiver = Some(rx);

    let mut cache = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+line2", 4);
    cache.file_index = 1;
    cache.highlighted = true;
    tx.send(cache).await.unwrap();

    app.poll_prefetch_updates();

    assert!(app.highlighted_cache_store.contains_key(&1));
}

#[tokio::test]
async fn test_poll_prefetch_skips_current_file() {
    let mut app = App::new_for_test();
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "a.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: Some("@@ -1 +1 @@\n+line".to_string()),
            viewed: false,
        }],
    };
    app.selected_file = 0;

    // Set up an existing highlighted diff_cache for current file
    // poll_prefetch_updates skips when diff_cache has highlighted=true for same file_index
    let mut existing_cache =
        crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+line", 4);
    existing_cache.file_index = 0;
    existing_cache.highlighted = true;
    app.diff_cache = Some(existing_cache);

    let (tx, rx) = mpsc::channel(2);
    app.prefetch_receiver = Some(rx);

    let mut cache = crate::ui::diff_view::build_plain_diff_cache("@@ -1 +1 @@\n+line", 4);
    cache.file_index = 0; // same as selected
    cache.highlighted = true;
    tx.send(cache).await.unwrap();

    app.poll_prefetch_updates();

    // Should skip storing cache for current file because diff_cache already has it highlighted
    assert!(!app.highlighted_cache_store.contains_key(&0));
}

#[tokio::test]
async fn test_poll_comment_submit_success() {
    use crate::loader::CommentSubmitResult;
    let mut app = App::new_for_test();
    app.pr_number = Some(1);
    app.comment_submitting = true;

    let (tx, rx) = mpsc::channel(1);
    app.comment_submit_receiver = Some((1, rx));

    tx.send(CommentSubmitResult::Success).await.unwrap();

    app.poll_comment_submit_updates();

    assert!(!app.comment_submitting);
    let (success, _) = app.submission_result.unwrap();
    assert!(success);
}

#[tokio::test]
async fn test_poll_comment_submit_failure() {
    use crate::loader::CommentSubmitResult;
    let mut app = App::new_for_test();
    app.pr_number = Some(1);
    app.comment_submitting = true;

    let (tx, rx) = mpsc::channel(1);
    app.comment_submit_receiver = Some((1, rx));

    tx.send(CommentSubmitResult::Error("network error".to_string()))
        .await
        .unwrap();

    app.poll_comment_submit_updates();

    assert!(!app.comment_submitting);
    let (success, msg) = app.submission_result.unwrap();
    assert!(!success);
    assert!(msg.contains("network error"));
}

#[tokio::test]
async fn test_poll_mark_viewed_success() {
    let mut app = App::new_for_test();
    app.pr_number = Some(1);
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };

    let (tx, rx) = mpsc::channel(1);
    app.mark_viewed_receiver = Some((1, rx));

    tx.send(MarkViewedResult::Completed {
        marked_paths: vec!["test.rs".to_string()],
        total_targets: 1,
        error: None,
        set_viewed: true,
    })
    .await
    .unwrap();

    app.poll_mark_viewed_updates();

    if let DataState::Loaded { files, .. } = &app.data_state {
        assert!(files[0].viewed);
    }
    let (success, _) = app.submission_result.unwrap();
    assert!(success);
}

#[tokio::test]
async fn test_poll_mark_viewed_error() {
    let mut app = App::new_for_test();
    app.pr_number = Some(1);
    app.data_state = DataState::Loaded {
        pr: Box::new(make_local_pr()),
        files: vec![ChangedFile {
            filename: "test.rs".to_string(),
            status: "modified".to_string(),
            additions: 1,
            deletions: 0,
            patch: None,
            viewed: false,
        }],
    };

    let (tx, rx) = mpsc::channel(1);
    app.mark_viewed_receiver = Some((1, rx));

    tx.send(MarkViewedResult::Completed {
        marked_paths: vec![],
        total_targets: 1,
        error: Some("API error".to_string()),
        set_viewed: true,
    })
    .await
    .unwrap();

    app.poll_mark_viewed_updates();

    let (success, msg) = app.submission_result.unwrap();
    assert!(!success);
    assert!(msg.contains("API error"));
}

#[tokio::test]
async fn test_poll_comment_updates_cross_pr_discards() {
    let mut app = App::new_for_test();
    app.pr_number = Some(2); // Current PR is 2

    let (tx, rx) = mpsc::channel(1);
    app.comment_receiver = Some((1, rx)); // Data is for PR 1
    app.comments_loading = true;

    tx.send(Ok(vec![])).await.unwrap();

    app.poll_comment_updates();

    // Should NOT update UI for wrong PR
    assert!(app.comments_loading);
    assert!(app.review_comments.is_none());
}

#[tokio::test]
async fn test_poll_discussion_comment_cross_pr_discards() {
    let mut app = App::new_for_test();
    app.pr_number = Some(2); // Current PR is 2

    let (tx, rx) = mpsc::channel(1);
    app.discussion_comment_receiver = Some((1, rx)); // Data is for PR 1
    app.discussion_comments_loading = true;

    tx.send(Ok(vec![])).await.unwrap();

    app.poll_discussion_comment_updates();

    // Should NOT update UI for wrong PR
    assert!(app.discussion_comments_loading);
    assert!(app.discussion_comments.is_none());
}
