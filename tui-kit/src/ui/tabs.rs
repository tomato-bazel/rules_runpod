//! Tab bar renderer.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

use crate::theme;

pub fn render(frame: &mut Frame, area: Rect, titles: &[&str], active: usize) {
    let spans: Vec<Line> = titles
        .iter()
        .map(|t| Line::from(Span::raw(*t)))
        .collect();
    let tabs = Tabs::new(spans)
        .select(active)
        .block(Block::default().borders(Borders::BOTTOM))
        .style(theme::title_inactive())
        .highlight_style(theme::title_active())
        .divider("│");
    frame.render_widget(tabs, area);
}
