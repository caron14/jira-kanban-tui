use crate::domain::dashboard::{AttentionItem, DashboardStats, Workload};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_dashboard(
    frame: &mut Frame,
    area: Rect,
    stats: &DashboardStats,
    attention: &[AttentionItem],
    workload: &[Workload],
    selected: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(7), Constraint::Length(7)])
        .split(area);

    let progress_width = 20usize;
    let filled = ((stats.progress_pct / 100.0) * progress_width as f64).round() as usize;
    let bar = format!(
        "{}{}",
        "█".repeat(filled.min(progress_width)),
        "░".repeat(progress_width.saturating_sub(filled))
    );
    let stats_text = format!(
        "Progress {bar} {:>3.0}%\nOpen {}   In progress {}   Done {}   Blocked {}   Overdue {}   Due today {}",
        stats.progress_pct,
        stats.open,
        stats.in_progress,
        stats.done,
        stats.blocked,
        stats.overdue,
        stats.due_today,
    );
    frame.render_widget(
        Paragraph::new(stats_text)
            .block(Block::default().borders(Borders::ALL).title(" Project health ")),
        chunks[0],
    );

    let visible = usize::from(chunks[1].height.saturating_sub(2));
    let scroll = selected.saturating_sub(visible.saturating_sub(1));
    let lines = attention
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = match item.kind {
                crate::domain::dashboard::AttentionKind::Overdue => "OVERDUE",
                crate::domain::dashboard::AttentionKind::Blocked => "BLOCKED",
                crate::domain::dashboard::AttentionKind::DueToday => "DUE TODAY",
                crate::domain::dashboard::AttentionKind::DueSoon => "DUE SOON",
                crate::domain::dashboard::AttentionKind::Stale => "STALE",
                crate::domain::dashboard::AttentionKind::Unassigned => "UNASSIGNED",
                crate::domain::dashboard::AttentionKind::NoDueDate => "NO DUE DATE",
            };
            let line = Line::from(vec![
                Span::styled(
                    format!("{:<11}", marker),
                    Style::default().fg(if index == selected { Color::Yellow } else { Color::Red }),
                ),
                Span::styled(item.issue.key.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(format!("  {}", item.issue.summary)),
            ]);
            if index == selected {
                line.style(Style::default().bg(Color::DarkGray))
            } else {
                line
            }
        })
        .collect::<Vec<_>>();
    let attention_widget = if lines.is_empty() {
        Paragraph::new("Nothing needs attention")
    } else {
        Paragraph::new(lines).scroll((scroll as u16, 0))
    };
    frame.render_widget(
        attention_widget.block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Attention · Enter details · j/k select "),
        ),
        chunks[1],
    );

    let workload_lines = workload
        .iter()
        .take(usize::from(chunks[2].height.saturating_sub(2)))
        .map(|item| {
            Line::raw(format!(
                "{:<20} total {:>3}  todo {:>3}  doing {:>3}  blocked {:>2}  overdue {:>2}",
                item.assignee, item.total, item.todo, item.doing, item.blocked, item.overdue
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(workload_lines)
            .block(Block::default().borders(Borders::ALL).title(" Workload by assignee ")),
        chunks[2],
    );
}
