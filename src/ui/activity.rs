use crate::domain::activity::Activity;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::layout::SelectableListRegion;

pub fn hit_test(
    area: Rect,
    item_count: usize,
    selected: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    SelectableListRegion { area, item_count, selected, row_height: 1 }.hit(column, row)
}

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
    let scroll =
        SelectableListRegion { area, item_count: activities.len(), selected, row_height: 1 }
            .scroll();
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
