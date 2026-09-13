use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSections {
    pub header: Rect,
    pub content: Rect,
    pub footer: Rect,
}

impl AppSections {
    pub fn new(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(10), Constraint::Length(1)])
            .split(area);
        Self { header: chunks[0], content: chunks[1], footer: chunks[2] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitRegion<T> {
    pub area: Rect,
    pub target: T,
}

impl<T: Copy> HitRegion<T> {
    pub fn hit(&self, column: u16, row: u16) -> Option<T> {
        contains(self.area, column, row).then_some(self.target)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectableListRegion {
    pub area: Rect,
    pub item_count: usize,
    pub selected: usize,
    pub row_height: usize,
}

impl SelectableListRegion {
    pub fn scroll(&self) -> usize {
        self.selected.saturating_sub(self.visible_rows().saturating_sub(1))
    }

    pub fn hit(&self, column: u16, row: u16) -> Option<usize> {
        let inside = column > self.area.x
            && column < self.area.x.saturating_add(self.area.width).saturating_sub(1)
            && row > self.area.y
            && row < self.area.y.saturating_add(self.area.height).saturating_sub(1);
        if !inside {
            return None;
        }
        let index = self.scroll() + usize::from(row - self.area.y - 1) / self.row_height.max(1);
        (index < self.item_count).then_some(index)
    }

    fn visible_rows(&self) -> usize {
        usize::from(self.area.height.saturating_sub(2)) / self.row_height.max(1)
    }
}

pub fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectable_list_uses_the_same_scroll_for_rendering_and_hits() {
        let region = SelectableListRegion {
            area: Rect::new(10, 5, 30, 6),
            item_count: 10,
            selected: 7,
            row_height: 1,
        };

        assert_eq!(region.scroll(), 4);
        assert_eq!(region.hit(12, 6), Some(4));
        assert_eq!(region.hit(12, 9), Some(7));
        assert_eq!(region.hit(10, 6), None);
    }
}
