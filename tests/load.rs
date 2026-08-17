#![allow(clippy::field_reassign_with_default)]

use jira_kanban_tui::app::state::AppState;
use jira_kanban_tui::domain::{Board, BoardColumn, Issue, IssueType};
use std::time::Duration;

const NAVIGATION_BUDGET: Duration = Duration::from_millis(250);

fn big_board(n: usize) -> (Board, Vec<Issue>) {
    let board = Board {
        id: 1,
        name: "Big".into(),
        columns: vec![
            BoardColumn { name: "To Do".into(), statuses: vec!["To Do".into()] },
            BoardColumn { name: "In Progress".into(), statuses: vec!["In Progress".into()] },
            BoardColumn { name: "Done".into(), statuses: vec!["Done".into()] },
        ],
    };
    let issues = (0..n)
        .map(|i| Issue {
            key: format!("P-{}", i),
            summary: format!("Issue {} with some summary text for truncation test", i),
            issue_type: IssueType::Task,
            status: match i % 3 {
                0 => "To Do",
                1 => "In Progress",
                _ => "Done",
            }
            .into(),
            assignee: None,
            priority: None,
            due_date: None,
            updated: None,
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked: i % 10 == 0,
            overdue: i % 7 == 0,
        })
        .collect();
    (board, issues)
}

#[test]
fn large_board_navigation_and_filter() {
    let (board, issues) = big_board(500);
    let mut state = AppState::default();
    state.board = Some(board);
    state.issues = issues;
    state.apply_filters();

    // navigation
    let start = std::time::Instant::now();
    for _ in 0..500 {
        state.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
    let elapsed = start.elapsed();
    assert!(elapsed < NAVIGATION_BUDGET, "navigation too slow: {elapsed:?}");

    // filter
    let start = std::time::Instant::now();
    state.search_query = Some("P-1".into());
    state.apply_filters();
    assert!(start.elapsed().as_millis() < 50);
    assert!(!state.filtered_issues.is_empty());

    // refresh flag
    state.refreshing = true;
    state.refreshing = false;
}

#[test]
fn large_board_render() {
    use ratatui::{backend::TestBackend, Terminal};
    let (board, issues) = big_board(300);
    let mut state = AppState::default();
    state.board = Some(board);
    state.issues = issues;
    state.apply_filters();
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let start = std::time::Instant::now();
    terminal.draw(|f| jira_kanban_tui::ui::render(f, &state)).unwrap();
    assert!(start.elapsed().as_millis() < 50, "render too slow: {:?}", start.elapsed());
}
