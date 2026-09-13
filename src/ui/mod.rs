pub mod activity;
pub mod board;
pub mod dashboard;
pub mod layout;
pub mod setup;
pub mod wbs;

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::state::{AppState, Modal, NetworkState, View};
use layout::{contains, HitRegion, SelectableListRegion};

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalHit {
    DetailField(usize),
    ListItem(usize),
    Outside,
}

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

    let sections = layout::AppSections::new(frame.area());
    render_header(frame, sections.header, state);
    render_content(frame, sections.content, state);
    render_footer(frame, sections.footer, state);

    match state.modal {
        Modal::Help => render_help(frame, state),
        Modal::Detail => render_detail(frame, state),
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
            &format!("Assignee · type to search: {}", state.input_buffer),
            assignee_items(state),
            state.picker_index,
            16,
        ),
        Modal::DueDatePicker => {
            render_list(frame, "Due date", due_date_items(), state.picker_index, 9)
        }
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

pub fn modal_hit_test(area: Rect, state: &AppState, column: u16, row: u16) -> Option<ModalHit> {
    let list = modal_list_spec(state);
    let modal_area = if let Some((item_count, _, max_height)) = list {
        list_area(item_count, max_height, area)
    } else {
        match state.modal {
            Modal::Detail => centered(82, 18, area),
            Modal::DueDateEditor | Modal::Search => centered(72, 6, area),
            Modal::Error => centered(74, 9, area),
            Modal::Help => centered(76, 17, area),
            Modal::None => return None,
            _ => return None,
        }
    };
    if !contains(modal_area, column, row) {
        return Some(ModalHit::Outside);
    }
    if state.modal == Modal::Detail {
        let first_field_row = modal_area.y.saturating_add(3);
        if column > modal_area.x
            && column < modal_area.x.saturating_add(modal_area.width).saturating_sub(1)
            && row >= first_field_row
            && row < first_field_row.saturating_add(4)
        {
            return Some(ModalHit::DetailField(usize::from(row - first_field_row)));
        }
    }
    if let Some((item_count, selected, _)) = list {
        if let Some(index) = list_region(modal_area, item_count, selected).hit(column, row) {
            return Some(ModalHit::ListItem(index));
        }
    }
    None
}

fn modal_list_spec(state: &AppState) -> Option<(usize, usize, u16)> {
    match state.modal {
        Modal::TransitionPicker => Some((state.transitions.len(), state.picker_index, 14)),
        Modal::AssigneePicker => Some((assignee_items(state).len(), state.picker_index, 16)),
        Modal::DueDatePicker => Some((5, state.picker_index, 9)),
        Modal::PriorityPicker => Some((state.choices.len(), state.picker_index, 14)),
        Modal::BoardPicker => Some((state.board_refs.len(), state.picker_index, 16)),
        Modal::Filter => Some((4, state.filter_index, 9)),
        _ => None,
    }
}

fn assignee_items(state: &AppState) -> Vec<String> {
    let mut items = Vec::new();
    if state.current_user.is_some() {
        items.push("Assign to me".into());
    }
    items.push("Unassign".into());
    items.extend(state.choices.iter().map(|item| item.label.clone()));
    items
}

fn due_date_items() -> Vec<String> {
    let today = chrono::Local::now().date_naive();
    vec![
        format!("Today · {today}"),
        format!("Tomorrow · {}", today + chrono::Duration::days(1)),
        format!("One week from today · {}", today + chrono::Duration::days(7)),
        "Clear due date".into(),
        "Enter a date…".into(),
    ]
}

fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let labels = header_labels();
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
        Paragraph::new(format!(" jira-kanban-tui · {board_name}"))
            .style(Style::default().fg(Color::White).bg(Color::DarkGray)),
        Rect::new(area.x, area.y, area.width, 1.min(area.height)),
    );
    frame.render_widget(
        Paragraph::new(Line::from(tabs)),
        Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1)),
    );
}

pub fn header_hit_test(area: Rect, column: u16, row: u16) -> Option<View> {
    let views = [View::Board, View::Dashboard, View::Wbs, View::Activity];
    let mut x = area.x;
    let tab_row =
        Rect::new(area.x, area.y.saturating_add(1), area.width, area.height.saturating_sub(1));
    for (label, view) in header_labels().into_iter().zip(views) {
        let width = label.len() as u16 + 2;
        let region = HitRegion {
            area: Rect::new(
                x,
                tab_row.y,
                width.min(tab_row.x.saturating_add(tab_row.width).saturating_sub(x)),
                tab_row.height,
            ),
            target: view,
        };
        if let Some(view) = region.hit(column, row) {
            return Some(view);
        }
        x = x.saturating_add(width);
        if x >= area.x.saturating_add(area.width) {
            break;
        }
    }
    None
}

fn header_labels() -> [&'static str; 4] {
    ["1 Board", "2 Dashboard", "3 WBS", "4 Activity"]
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
            let workload =
                crate::domain::dashboard::workload_by_assignee(&state.issues, &done, &progress);
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
            wbs::render_wbs(frame, area, &state.wbs_roots, &state.expanded, state.wbs_selected);
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
    } else if state.view == View::Activity && state.activity_loading {
        "Loading Activity…".into()
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
                    format!("AUTHENTICATION ERROR · READ-ONLY · {message} · s repair connection")
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
    let Some(issue) = state.detail_issue() else { return };
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
    let field = |index: usize, label: &str, value: &str| {
        let selected = !state.offline && state.edit_index == index;
        let line = Line::raw(format!("{} {label:<11} {value}", if selected { "▶" } else { " " }));
        if selected {
            line.style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
        } else {
            line
        }
    };
    let lines = vec![
        Line::raw(issue.summary.clone()),
        Line::raw(""),
        field(0, "Status", &issue.status),
        field(1, "Assignee", assignee),
        field(2, "Due", &due),
        field(3, "Priority", priority),
        Line::raw(format!(
            "  {:<11} {}",
            "Parent",
            issue.parent_key.as_deref().or(issue.epic_key.as_deref()).unwrap_or("—")
        )),
        Line::raw(format!("  {:<11} {dependencies}", "Dependencies")),
        Line::raw(format!("  {:<11} {updated}", "Updated")),
        Line::raw(""),
        Line::raw(if state.offline {
            "Read-only cache · o open Jira · Esc close"
        } else {
            "↑/↓ select · Enter edit · o open Jira · Esc close"
        }),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(format!(" {} ", issue.key))),
        area,
    );
}

fn render_list(
    frame: &mut Frame,
    title: &str,
    items: Vec<String>,
    selected: usize,
    max_height: u16,
) {
    let area = list_area(items.len(), max_height, frame.area());
    frame.render_widget(Clear, area);
    let scroll = list_region(area, items.len(), selected).scroll();
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

fn list_area(item_count: usize, max_height: u16, area: Rect) -> Rect {
    let height = (item_count as u16 + 2).clamp(5, max_height);
    centered(66, height, area)
}

fn list_region(area: Rect, item_count: usize, selected: usize) -> SelectableListRegion {
    SelectableListRegion { area, item_count, selected, row_height: 1 }
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
    if state.detail_issue().or_else(|| state.selected_issue()).is_some() {
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
    let edit_help =
        if state.offline { "" } else { "  e             open editable Issue details\n" };
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
