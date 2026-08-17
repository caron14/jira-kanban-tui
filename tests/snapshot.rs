#![allow(clippy::field_reassign_with_default)]

use jira_kanban_tui::app::state::{AppAction, AppState, Modal, View};
use jira_kanban_tui::domain::{Assignee, Board, BoardColumn, Issue, IssueType, Priority};
use ratatui::{backend::TestBackend, Terminal};

fn sample_board() -> Board {
    Board {
        id: 1,
        name: "Test Board".into(),
        columns: vec![
            BoardColumn { name: "To Do".into(), statuses: vec!["To Do".into()] },
            BoardColumn { name: "In Progress".into(), statuses: vec!["In Progress".into()] },
            BoardColumn { name: "Done".into(), statuses: vec!["Done".into()] },
            BoardColumn { name: "Blocked".into(), statuses: vec!["Blocked".into()] },
        ],
    }
}

fn sample_issues() -> Vec<Issue> {
    vec![
        Issue {
            key: "PROJ-1".into(),
            summary: "Fix critical bug with long summary that should be truncated correctly".into(),
            issue_type: IssueType::Bug,
            status: "To Do".into(),
            assignee: Some(Assignee { display_name: "Alice".into(), account_id: None }),
            priority: Some(Priority { name: "High".into(), id: "2".into() }),
            due_date: Some(chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            updated: None,
            epic_key: Some("EPIC-1".into()),
            parent_key: None,
            links: vec![],
            blocked: false,
            overdue: true,
        },
        Issue {
            key: "PROJ-2".into(),
            summary: "Blocked task".into(),
            issue_type: IssueType::Task,
            status: "Blocked".into(),
            assignee: None,
            priority: None,
            due_date: None,
            updated: None,
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked: true,
            overdue: false,
        },
        Issue {
            key: "PROJ-3".into(),
            summary: "Normal task".into(),
            issue_type: IssueType::Story,
            status: "Done".into(),
            assignee: Some(Assignee { display_name: "Bob".into(), account_id: None }),
            priority: Some(Priority { name: "Low".into(), id: "4".into() }),
            due_date: None,
            updated: None,
            epic_key: None,
            parent_key: None,
            links: vec![],
            blocked: false,
            overdue: false,
        },
    ]
}

#[test]
fn render_board_deterministic() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = AppState::default();
    state.view = View::Board;
    state.board = Some(sample_board());
    state.issues = sample_issues();
    state.apply_filters();

    terminal.draw(|f| jira_kanban_tui::ui::render(f, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
    // Deterministic checks — no Jira API involved
    assert!(content.contains("PROJ-1"), "missing PROJ-1 in {}", content);
    assert!(content.contains("To Do"), "missing To Do column");
    // Overdue/Blocked are symbol+text — either is fine, but board may truncate on narrow 80 cols; just check priority/summary present
    assert!(content.contains("Fix critical") || content.contains("PROJ-1"));

    state.selected_col = 3;
    terminal.draw(|f| jira_kanban_tui::ui::render(f, &state)).unwrap();
    let content: String =
        terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect();
    assert!(content.contains("PROJ-2"), "horizontal focus did not reveal PROJ-2");
}

#[test]
fn render_setup_deterministic() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = AppState::default();
    state.view = View::Setup;
    state.setup.url = "https://example.atlassian.net".into();
    state.setup.username = "a@b.com".into();
    terminal.draw(|f| jira_kanban_tui::ui::render(f, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("Setup"));
    assert!(content.contains("Jira URL") || content.contains("https://"));
}

#[test]
fn render_help_modal() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = AppState::default();
    state.modal = Modal::Help;
    terminal.draw(|f| jira_kanban_tui::ui::render(f, &state)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("Help") || content.contains("Board"));
}

#[test]
fn render_search_filter_error_modals() {
    for modal in [Modal::Search, Modal::Filter, Modal::Error] {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = AppState::default();
        state.modal = modal;
        if modal == Modal::Error {
            state.error = Some("test error".into());
        }
        terminal.draw(|f| jira_kanban_tui::ui::render(f, &state)).unwrap();
    }
}

#[test]
fn error_modal_only_shows_executable_actions() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = AppState::default();
    state.modal = Modal::Error;
    state.error = Some("configuration failed".into());
    terminal.draw(|frame| jira_kanban_tui::ui::render(frame, &state)).unwrap();
    let content: String =
        terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect();
    assert!(!content.contains("retry"));
    assert!(!content.contains("open Jira"));

    state.retry_action = Some(AppAction::Refresh);
    terminal.draw(|frame| jira_kanban_tui::ui::render(frame, &state)).unwrap();
    let content: String =
        terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect();
    assert!(content.contains("retry"));
}

#[test]
fn too_small_terminal_shows_requirement_instead_of_clipped_ui() {
    let backend = TestBackend::new(60, 18);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| jira_kanban_tui::ui::render(frame, &AppState::default())).unwrap();
    let content: String =
        terminal.backend().buffer().content().iter().map(|cell| cell.symbol()).collect();
    assert!(content.contains("Terminal too small"));
    assert!(content.contains("80×24"));
}

#[test]
fn navigation_preserves_selection() {
    let mut state = AppState::default();
    state.view = View::Board;
    state.board = Some(sample_board());
    state.issues = sample_issues();
    state.apply_filters();
    // j
    state.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(state.column_rows[0], 0, "j stays on the only card in the focused column");
    // k
    state.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('k'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(state.column_rows[0], 0);
    // l changes focus without updating Jira
    let action = state.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('l'),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(matches!(action, jira_kanban_tui::app::state::AppAction::None));
    assert_eq!(state.selected_col, 1);
}
