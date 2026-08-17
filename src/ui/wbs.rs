use crate::domain::wbs::WbsNode;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_wbs(
    f: &mut Frame,
    area: Rect,
    roots: &[WbsNode],
    expanded: &std::collections::HashSet<String>,
    selected: usize,
) {
    let mut lines = Vec::new();
    let mut index = 0;
    for root in roots {
        render_node(&mut lines, root, expanded, 0, selected, &mut index);
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no hierarchy — check Epic/parent links)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let visible = usize::from(area.height.saturating_sub(2));
    let scroll = selected.saturating_sub(visible.saturating_sub(1));
    let p = Paragraph::new(lines).scroll((scroll as u16, 0)).block(
        Block::default().borders(Borders::ALL).title(" WBS · h/l collapse/expand · Enter details "),
    );
    f.render_widget(p, area);
}

fn render_node(
    lines: &mut Vec<Line>,
    node: &WbsNode,
    expanded: &std::collections::HashSet<String>,
    indent: usize,
    selected: usize,
    index: &mut usize,
) {
    let prefix = "  ".repeat(indent);
    let expand_icon = if node.children.is_empty() {
        "  "
    } else if expanded.contains(&node.issue.key) {
        "▼ "
    } else {
        "▶ "
    };
    let progress = format!("{:.0}%", node.progress);
    let line = Line::from(vec![
        Span::raw(prefix),
        Span::raw(expand_icon),
        Span::styled(node.issue.key.clone(), Style::default().fg(Color::Cyan)),
        Span::raw(format!(" {} [{}] {}", node.issue.summary, node.issue.status, progress)),
    ]);
    lines.push(if *index == selected {
        line.style(Style::default().bg(Color::DarkGray))
    } else {
        line
    });
    *index += 1;
    if expanded.contains(&node.issue.key) {
        for child in &node.children {
            render_node(lines, child, expanded, indent + 1, selected, index);
        }
    }
}
