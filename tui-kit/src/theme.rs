//! Centralised colors and styles. Single source of truth so views
//! stay visually consistent and any palette change is one edit.

use ratatui::style::{Color, Modifier, Style};

pub fn title_active() -> Style {
    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
}

pub fn title_inactive() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn header_row() -> Style {
    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

pub fn selected_row() -> Style {
    Style::default().bg(Color::Indexed(237)).add_modifier(Modifier::BOLD)
}

pub fn status_bar() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn error() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}
