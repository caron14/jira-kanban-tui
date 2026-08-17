pub mod activity;
pub mod board;
pub mod dashboard;
pub mod setup;
pub mod wbs;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::state::{AppState, Modal, NetworkState, View};

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

pub fn render(frame: &mut Frame, state: &AppState) {
    if frame.area().width < MIN_WIDTH || frame.area().height < MIN_HEIGHT {
        frame.render_widget(
            Paragraph::new(format!(
                "Terminal too small\n\nCurrent: {}×{}\nRequired: at least {MIN_WIDTH}×{MIN_HEIGHT}",
                frame.area().width,
                frame.area().height
            ))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" jira-kanban-tui ")),
            frame.area(),
        );
        return;
    }

    if state.view == View::Setup {
        setup::render_setup(frame, frame.area(), &state.setup);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(10), Constraint::Length(1)])
        .split(frame.area());
    render_header(frame, chunks[0], state);
    render_content(frame, chunks[1], state);
    render_footer(frame, chunks[2], state);

    match state.modal {
        Modal::Help => render_help(frame, state),
        Modal::Detail => render_detail(frame, state),
        Modal::EditMenu => render_list(frame, "Edit Issue", edit_items(), state.edit_index, 7),
        Modal::TransitionPicker => render_list(
            frame,
            "Select Status · Enter confirms",
            state
                .transitions
                .iter()
                .map(|item| format!("{} → {}", item.name, item.target_status))
                .collect(),
            state.picker_index,
            14,
        ),
        Modal::AssigneePicker => render_list(
            frame,
            &format!("Assignee · type to search: {} · Delete unassigns", state.input_buffer),
            state.choices.iter().map(|item| item.label.clone()).collect(),
            state.picker_index,
            16,
        ),
        Modal::PriorityPicker => render_list(
            frame,
            "Priority · Enter confirms",
            state.choices.iter().map(|item| item.label.clone()).collect(),
            state.picker_index,
            14,
        ),
        Modal::BoardPicker => render_list(
            frame,
            "Select Board",
            state
                .board_refs
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    format!(
                        "{} · Board #{id}",
                        state.board_names.get(index).map(String::as_str).unwrap_or(id)
                    )
                })
                .collect(),
            state.picker_index,
            16,
        ),
        Modal::DueDateEditor => render_input(
            frame,
            "Due date · YYYY-MM-DD · empty clears · Enter confirms",
            &state.input_buffer,
            state.error.as_deref(),
        ),
        Modal::Search => render_input(
            frame,
            "Search Key, Summary, Assignee · Enter keeps · Esc clears",
            &state.input_buffer,
            None,
        ),
        Modal::Filter => render_list(
            frame,
            "Filter",
            vec!["All".into(), "My Issues".into(), "Overdue".into(), "Blocked".into()],
            state.filter_index,
            9,
        ),
        Modal::Error => render_error(frame, state),
        Modal::None => {}
    }
}

fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let labels = ["1 Board", "2 Dashboard", "3 WBS", "4 Activity"];
    let active = match state.view {
        View::Board => 0,
        View::Dashboard => 1,
        View::Wbs => 2,
        View::Activity => 3,
        View::Setup => 0,
    };
    let tabs = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            if index == active {
                Span::styled(
                    format!(" {label} "),
                    Style::default().fg(Color::Yellow).bg(Color::DarkGray),
                )
            } else {
                Span::raw(format!(" {label} "))
            }
        })
        .collect::<Vec<_>>();
    let board_name =
        state.board_names.get(state.board_ref_index).map(String::as_str).unwrap_or("Loading Board");
    frame.render_widget(
        Paragraph::new(Line::from(tabs)).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(format!(" jira-kanban-tui · {board_name} ")),
        ),
        area,
    );
}

fn render_content(frame: &mut Frame, area: Rect, state: &AppState) {
    match state.view {
        View::Board => board::render_board(frame, area, state),
        View::Dashboard => {
            let done = state
                .board
                .as_ref()
                .and_then(|board| board.columns.last())
                .map(|column| column.statuses.clone())
                .unwrap_or_default();
            let progress = state
                .board
                .as_ref()
                .map(|board| {
                    if board.columns.len() > 2 {
                        board.columns[1..board.columns.len() - 1]
                            .iter()
                            .flat_map(|column| column.statuses.clone())
                            .collect()
                    } else {
                        Vec::new()
                    }
                })
                .unwrap_or_default();
            let stats = crate::domain::dashboard::compute_stats(&state.issues, &done, &progress);
            let attention = state.attention_items();
            let workload = crate::domain::dashboard::workload_by_assignee(&state.issues);
            dashboard::render_dashboard(
                frame,
                area,
                &stats,
                &attention,
                &workload,
                state.dashboard_selected,
            );
        }
        View::Wbs => {
            let done = state
                .board
                .as_ref()
                .and_then(|board| board.columns.last())
                .map(|column| column.statuses.clone())
                .unwrap_or_default();
            let roots = crate::domain::wbs::build_wbs(&state.issues, &done);
            wbs::render_wbs(frame, area, &roots, &state.expanded, state.wbs_selected);
        }
        View::Activity => {
            activity::render_activity(frame, area, &state.activities, state.activity_selected)
        }
        View::Setup => {}
    }
}

fn render_footer(frame: &mut Frame, area: Rect, state: &AppState) {
    let status = if state.loading {
        "Loading Board…".into()
    } else if state.refreshing {
        "Refreshing…".into()
    } else {
        let message = state.status_message.clone().unwrap_or_else(|| match state.view {
            View::Board => {
                "j/k issue  h/l column  Enter details  e edit  / search  f filter".into()
            }
            View::Dashboard => "j/k attention  Enter details  b Board  r refresh".into(),
            View::Wbs => "j/k issue  h/l collapse/expand  Enter details".into(),
            View::Activity => "j/k change  Enter details  r refresh".into(),
            View::Setup => String::new(),
        });
        if state.offline {
            match state.network {
                NetworkState::RateLimited => {
                    format!("RATE LIMITED · READ-ONLY · {message} · r retry")
                }
                NetworkState::AuthError => {
                    format!("AUTHENTICATION ERROR · READ-ONLY · {message} · run doctor")
                }
                _ => format!("READ-ONLY · {message} · r retry"),
            }
        } else {
            message
        }
    };
    frame.render_widget(
        Paragraph::new(format!(" {status}   ? help  q quit "))
            .style(Style::default().fg(Color::White).bg(Color::DarkGray)),
        area,
    );
}

fn render_detail(frame: &mut Frame, state: &AppState) {
    let area = centered(82, 18, frame.area());
    frame.render_widget(Clear, area);
    let Some(issue) = state.selected_issue() else { return };
    let assignee =
        issue.assignee.as_ref().map(|value| value.display_name.as_str()).unwrap_or("Unassigned");
    let priority = issue.priority.as_ref().map(|value| value.name.as_str()).unwrap_or("—");
    let due = issue.due_date.map(|value| value.to_string()).unwrap_or_else(|| "—".into());
    let updated = issue
        .updated
        .map(|value| value.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".into());
    let mut dependencies = Vec::new();
    for link in &issue.links {
        if !link.link_type.to_lowercase().contains("blocks") {
            continue;
        }
        if let Some(key) = &link.inward_issue {
            dependencies.push(format!("blocked by {key}"));
        }
        if let Some(key) = &link.outward_issue {
            dependencies.push(format!("blocks {key}"));
        }
    }
    let dependencies = if dependencies.is_empty() { "—".into() } else { dependencies.join(", ") };
    let text = format!(
        "{}\n\nStatus       {}\nAssignee     {}\nPriority     {}\nDue          {}\nParent       {}\nDependencies {}\nUpdated      {}\n\n{}",
        issue.summary,
        issue.status,
        assignee,
        priority,
        due,
        issue.parent_key.as_deref().or(issue.epic_key.as_deref()).unwrap_or("—"),
        dependencies,
        updated,
        if state.offline {
            "Read-only cache · o open Jira · Esc close"
        } else {
            "e edit · o open Jira · Esc close"
        }
    );
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(format!(" {} ", issue.key))),
        area,
    );
}

fn edit_items() -> Vec<String> {
    vec!["Status".into(), "Assignee".into(), "Due date".into(), "Priority".into()]
}

fn render_list(
    frame: &mut Frame,
    title: &str,
    items: Vec<String>,
    selected: usize,
    max_height: u16,
) {
    let height = (items.len() as u16 + 2).clamp(5, max_height);
    let area = centered(66, height, frame.area());
    frame.render_widget(Clear, area);
    let visible = usize::from(area.height.saturating_sub(2));
    let scroll = selected.saturating_sub(visible.saturating_sub(1));
    let lines = if items.is_empty() {
        vec![Line::styled("No choices", Style::default().fg(Color::DarkGray))]
    } else {
        items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let line =
                    Line::raw(format!("{} {item}", if index == selected { "▶" } else { " " }));
                if index == selected {
                    line.style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
                } else {
                    line
                }
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .block(Block::default().borders(Borders::ALL).title(format!(" {title} "))),
        area,
    );
}

fn render_input(frame: &mut Frame, title: &str, value: &str, error: Option<&str>) {
    let area = centered(72, 6, frame.area());
    frame.render_widget(Clear, area);
    let text = match error {
        Some(error) => format!("{value}_\n{error}"),
        None => format!("{value}_"),
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(format!(" {title} "))),
        area,
    );
}

fn render_error(frame: &mut Frame, state: &AppState) {
    let area = centered(74, 9, frame.area());
    frame.render_widget(Clear, area);
    let mut actions = vec!["Esc close"];
    if state.retry_action.is_some() {
        actions.insert(0, "r retry");
    }
    if state.selected_issue().is_some() {
        actions.insert(0, "o open Jira");
    }
    if state.network == NetworkState::AuthError {
        actions.insert(0, "s repair Setup");
    }
    frame.render_widget(
        Paragraph::new(format!(
            "{}\n\n{}",
            state.error.as_deref().unwrap_or("Unknown error"),
            actions.join(" · ")
        ))
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Error ")),
        area,
    );
}

fn render_help(frame: &mut Frame, state: &AppState) {
    let area = centered(76, 17, frame.area());
    frame.render_widget(Clear, area);
    let context = match state.view {
        View::Board => {
            "Board\n  j/k or ↑/↓   select Issue\n  h/l or ←/→   select Column\n  /             search\n  f             My Issues / Overdue / Blocked"
        }
        View::Dashboard => "Dashboard\n  j/k or ↑/↓   select Attention Issue\n  Enter         Issue details",
        View::Wbs => {
            "WBS\n  j/k or ↑/↓   select Issue\n  h/l or ←/→   collapse / expand\n  Enter         Issue details"
        }
        View::Activity => "Activity\n  j/k or ↑/↓   select change\n  Enter         Issue details",
        View::Setup => "",
    };
    let board_help = if state.board_refs.len() > 1 { "  b             select Board\n" } else { "" };
    let edit_help = if state.offline {
        ""
    } else {
        "  e             edit Status / Assignee / Due date / Priority\n"
    };
    let text = format!(
        "Global\n  1/2/3/4       Board / Dashboard / WBS / Activity\n{board_help}  r             refresh\n  ?             help\n  q / Ctrl+C    quit\n\n{context}\n\nIssue\n  Enter         details\n{edit_help}  o             open Jira\n\nEsc closes any dialog"
    );
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" Help ")),
        area,
    );
}

fn centered(percent_x: u16, height: u16, area: Rect) -> Rect {
    let width = (area.width.saturating_mul(percent_x) / 100).max(20).min(area.width);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height: height.min(area.height),
    }
}
