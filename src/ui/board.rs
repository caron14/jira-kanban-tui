use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::state::AppState;

const MIN_COLUMN_WIDTH: u16 = 24;
const CARD_HEIGHT: usize = 4;

pub fn render_board(frame: &mut Frame, area: Rect, state: &AppState) {
    let count = state.column_count();
    if count == 0 {
        frame.render_widget(
            Paragraph::new("No Board data · press r to refresh")
                .block(Block::default().borders(Borders::ALL).title(" Board ")),
            area,
        );
        return;
    }
    let visible_count = usize::from((area.width / MIN_COLUMN_WIDTH).max(1)).min(count);
    let start = if state.selected_col < state.col_scroll {
        state.selected_col
    } else if state.selected_col >= state.col_scroll + visible_count {
        state.selected_col + 1 - visible_count
    } else {
        state.col_scroll.min(count.saturating_sub(visible_count))
    };
    let end = (start + visible_count).min(count);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, (end - start) as u32); end - start])
        .split(area);

    for (offset, column_index) in (start..end).enumerate() {
        let issue_indices = state.column_issue_indices(column_index);
        let focused = column_index == state.selected_col;
        let title = format!(
            " {} ({}) ",
            state.column_label(column_index).unwrap_or_else(|| "Other".into()),
            issue_indices.len()
        );
        let block = Block::default().borders(Borders::ALL).title(title).border_style(if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });
        let width = usize::from(chunks[offset].width.saturating_sub(4)).max(1);
        let selected_row = *state.column_rows.get(column_index).unwrap_or(&0);
        let mut lines = Vec::new();
        for (row, issue_index) in issue_indices.iter().enumerate() {
            let issue = &state.issues[*issue_index];
            let selected = focused && row == selected_row;
            let updating = state.updating_key.as_deref() == Some(issue.key.as_str());
            let priority = issue.priority.as_ref().map(|value| value.name.as_str()).unwrap_or("");
            let top_text = if priority.is_empty() {
                format!("{}{}", if updating { "… " } else { "" }, issue.key)
            } else {
                format!("{}{} · {priority}", if updating { "… " } else { "" }, issue.key)
            };
            let assignee = issue
                .assignee
                .as_ref()
                .map(|value| value.display_name.as_str())
                .unwrap_or("Unassigned");
            let mut metadata = assignee.to_string();
            if let Some(due) = issue.due_date {
                metadata.push_str(&format!(" · {due}"));
            }
            if issue.blocked {
                metadata.push_str(" · BLOCKED");
            }
            if crate::domain::filter::is_overdue(issue) {
                metadata.push_str(" · OVERDUE");
            }
            let style = if selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };
            lines.extend([
                Line::styled(format!("╭{}", horizontal(&top_text, width)), style),
                Line::styled(
                    format!("│ {}", truncate_width(&issue.summary, width.saturating_sub(2))),
                    style,
                ),
                Line::styled(
                    format!("│ {}", truncate_width(&metadata, width.saturating_sub(2))),
                    style,
                ),
                Line::styled(format!("╰{}", "─".repeat(width)), style),
            ]);
        }
        if lines.is_empty() {
            lines.push(Line::styled("(empty)", Style::default().fg(Color::DarkGray)));
        }
        let visible_rows = usize::from(chunks[offset].height.saturating_sub(2)) / CARD_HEIGHT;
        let scroll_row = selected_row.saturating_sub(visible_rows.saturating_sub(1));
        frame.render_widget(
            Paragraph::new(lines).scroll(((scroll_row * CARD_HEIGHT) as u16, 0)).block(block),
            chunks[offset],
        );
    }
}

fn horizontal(text: &str, width: usize) -> String {
    let value = format!("─ {} ", truncate_width(text, width.saturating_sub(4)));
    let used = UnicodeWidthStr::width(value.as_str());
    format!("{value}{}", "─".repeat(width.saturating_sub(used)))
}

pub fn truncate_width(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    let target = max_width.saturating_sub(1);
    let mut used = 0;
    let mut output = String::new();
    for character in value.chars() {
        let width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + width > target {
            break;
        }
        used += width;
        output.push(character);
    }
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_by_terminal_width() {
        assert_eq!(truncate_width("日本語abc", 5), "日本…");
        assert_eq!(UnicodeWidthStr::width(truncate_width("日本語abc", 5).as_str()), 5);
    }
}
