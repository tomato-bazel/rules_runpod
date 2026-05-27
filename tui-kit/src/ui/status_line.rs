//! Bottom status bar — fixed-width help text + optional right-aligned
//! status (e.g., "loading…", "error: …").

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Span,
    widgets::Paragraph,
};

use crate::theme;

pub fn render(frame: &mut Frame, area: Rect, help: &str, status: Option<&str>) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(40)])
        .split(area);
    frame.render_widget(
        Paragraph::new(Span::styled(help, theme::status_bar())),
        cols[0],
    );
    if let Some(s) = status {
        let style = if s.starts_with("error") {
            theme::error()
        } else {
            theme::status_bar()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(s, style)).alignment(ratatui::layout::Alignment::Right),
            cols[1],
        );
    }
}
