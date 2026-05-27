//! Detail pane — right-hand side of a split list/detail layout.
//!
//! Stateless renderer: caller passes the title + (label, value) pairs
//! and we lay them out as label-padded rows. Long values wrap.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::theme;

pub fn render(frame: &mut Frame, area: Rect, title: &str, pairs: &[(String, String)]) {
    let label_width = pairs
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0)
        .min(24);

    let lines: Vec<Line> = pairs
        .iter()
        .map(|(k, v)| {
            let truncated: String = k.chars().take(label_width).collect();
            Line::from(vec![
                Span::styled(
                    format!("{:>width$}", truncated, width = label_width),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(v.clone(), theme::status_bar()),
            ])
        })
        .collect();

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} ")),
        );
    frame.render_widget(para, area);
}
