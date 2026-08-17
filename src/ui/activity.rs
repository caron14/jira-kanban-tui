use crate::domain::activity::Activity;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_activity(frame: &mut Frame, area: Rect, activities: &[Activity], selected: usize) {
    let lines = activities
        .iter()
        .enumerate()
        .map(|(index, activity)| {
            let line = Line::from(vec![
                Span::styled(
                    activity.at.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(activity.key.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(format!(
                    "  {:?}: {} → {}  {}",
                    activity.kind,
                    activity.from.as_deref().unwrap_or("—"),
                    activity.to.as_deref().unwrap_or("—"),
                    activity.summary
                )),
            ]);
            if index == selected {
                line.style(Style::default().bg(Color::DarkGray))
            } else {
                line
            }
        })
        .collect::<Vec<_>>();
    let visible = usize::from(area.height.saturating_sub(2));
    let scroll = selected.saturating_sub(visible.saturating_sub(1));
    let paragraph = if lines.is_empty() {
        Paragraph::new("No changes since yesterday")
    } else {
        Paragraph::new(lines).scroll((scroll as u16, 0))
    };
    frame.render_widget(
        paragraph.block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Activity since yesterday · Enter details "),
        ),
        area,
    );
}
