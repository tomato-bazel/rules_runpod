//! Confirm modal — a centered floating box with a yes/no prompt.
//!
//! Stateless renderer: the view owns the "modal is open" boolean and
//! the prompt text. Keystroke routing (y / n / Esc) is also up to the
//! view, since different prompts may want different keys.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Render a centered confirmation modal over `area`. Caller has
/// already decided to open it; this just draws the chrome + prompt.
pub fn render_confirm(frame: &mut Frame, area: Rect, prompt: &str) {
    let modal_area = centered_rect(area, 60, 7);
    frame.render_widget(Clear, modal_area); // wipe what's underneath

    let body = Paragraph::new(vec![
        Line::from(Span::raw(prompt)),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" confirm   "),
            Span::styled("[n / Esc]", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" cancel"),
        ]),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Confirm "),
    );
    frame.render_widget(body, modal_area);
}

/// Compute a centered Rect with the given width/height — both can be
/// either a fixed number of cells (≥10) or a percentage (1..100).
fn centered_rect(area: Rect, width_pct_or_cells: u16, height_cells: u16) -> Rect {
    let h = height_cells.min(area.height.saturating_sub(2));
    let w = if width_pct_or_cells <= 100 {
        (area.width as u32 * width_pct_or_cells as u32 / 100) as u16
    } else {
        width_pct_or_cells.min(area.width)
    };
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height - h) / 2),
            Constraint::Length(h),
            Constraint::Min(0),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width - w) / 2),
            Constraint::Length(w),
            Constraint::Min(0),
        ])
        .split(vertical[1]);
    horizontal[1]
}
