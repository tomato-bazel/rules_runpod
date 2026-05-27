//! Generic form widget for the TUI.
//!
//! Two field kinds: `Text` (free-form text input) and `Choice` (cycle
//! through a fixed set of options). Forms render as a centered modal
//! overlay with focus management and a single-line status footer.
//!
//! Keymap:
//!   Tab / ↓        next field
//!   Shift-Tab / ↑  previous field
//!   ← → home end   cursor in text fields; cycle in choice fields
//!   backspace      delete char left of cursor (text)
//!   any printable  insert at cursor (text)
//!   Enter          submit
//!   Esc            cancel

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme;

pub struct Form {
    pub title: String,
    fields: Vec<Field>,
    focus: usize,
    pub status: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum FormOutcome {
    None,
    Submit,
    Cancel,
}

pub enum Field {
    Text(TextField),
    Choice(ChoiceField),
}

pub struct TextField {
    pub label: &'static str,
    value: String,
    cursor: usize,
}

pub struct ChoiceField {
    pub label: &'static str,
    pub options: Vec<&'static str>,
    selected: usize,
}

impl TextField {
    pub fn new(label: &'static str, initial: impl Into<String>) -> Self {
        let value = initial.into();
        let cursor = value.chars().count();
        Self { label, value, cursor }
    }

    pub fn value(&self) -> &str { &self.value }

    fn insert(&mut self, c: char) {
        let byte_idx = char_to_byte(&self.value, self.cursor);
        self.value.insert(byte_idx, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 { return; }
        let start = char_to_byte(&self.value, self.cursor - 1);
        let end = char_to_byte(&self.value, self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    fn delete(&mut self) {
        let n = self.value.chars().count();
        if self.cursor >= n { return; }
        let start = char_to_byte(&self.value, self.cursor);
        let end = char_to_byte(&self.value, self.cursor + 1);
        self.value.replace_range(start..end, "");
    }

    fn move_left(&mut self) {
        if self.cursor > 0 { self.cursor -= 1; }
    }

    fn move_right(&mut self) {
        let n = self.value.chars().count();
        if self.cursor < n { self.cursor += 1; }
    }

    fn home(&mut self) { self.cursor = 0; }
    fn end(&mut self) { self.cursor = self.value.chars().count(); }
}

impl ChoiceField {
    pub fn new(label: &'static str, options: Vec<&'static str>, default: usize) -> Self {
        let selected = default.min(options.len().saturating_sub(1));
        Self { label, options, selected }
    }

    pub fn value(&self) -> &str { self.options[self.selected] }

    fn next(&mut self) {
        self.selected = (self.selected + 1) % self.options.len();
    }

    fn prev(&mut self) {
        self.selected = (self.selected + self.options.len() - 1) % self.options.len();
    }
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

impl Form {
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self { title: title.into(), fields, focus: 0, status: None }
    }

    /// Number of fields. Stable across the form's lifetime.
    pub fn len(&self) -> usize { self.fields.len() }

    /// Read the value of field at `idx` as a string, regardless of
    /// whether it's a Text or Choice field.
    pub fn value(&self, idx: usize) -> Option<&str> {
        match self.fields.get(idx)? {
            Field::Text(t) => Some(t.value()),
            Field::Choice(c) => Some(c.value()),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> FormOutcome {
        match key.code {
            KeyCode::Esc => return FormOutcome::Cancel,
            KeyCode::Enter => return FormOutcome::Submit,
            KeyCode::Tab | KeyCode::Down => self.focus_next(),
            KeyCode::BackTab | KeyCode::Up => self.focus_prev(),
            _ => self.dispatch_to_field(key),
        }
        FormOutcome::None
    }

    fn focus_next(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + 1) % self.fields.len();
        }
    }

    fn focus_prev(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + self.fields.len() - 1) % self.fields.len();
        }
    }

    fn dispatch_to_field(&mut self, key: KeyEvent) {
        let Some(field) = self.fields.get_mut(self.focus) else { return; };
        match field {
            Field::Text(t) => match key.code {
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => t.insert(c),
                KeyCode::Backspace => t.backspace(),
                KeyCode::Delete => t.delete(),
                KeyCode::Left => t.move_left(),
                KeyCode::Right => t.move_right(),
                KeyCode::Home => t.home(),
                KeyCode::End => t.end(),
                _ => {}
            },
            Field::Choice(c) => match key.code {
                KeyCode::Left => c.prev(),
                KeyCode::Right | KeyCode::Char(' ') => c.next(),
                _ => {}
            },
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        // Modal box: 70% wide, height proportional to fields.
        let height = (self.fields.len() as u16 * 2 + 5).min(area.height.saturating_sub(2));
        let modal = centered_rect(area, 70, height);
        frame.render_widget(Clear, modal);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", self.title));
        let inner = block.inner(modal);
        frame.render_widget(block, modal);

        // Layout: one row per field, then a footer.
        let mut constraints: Vec<Constraint> =
            (0..self.fields.len()).map(|_| Constraint::Length(2)).collect();
        constraints.push(Constraint::Min(1));   // spacer
        constraints.push(Constraint::Length(1)); // footer
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        for (i, field) in self.fields.iter().enumerate() {
            let focused = i == self.focus;
            render_field(frame, rows[i], field, focused);
        }

        let footer_idx = rows.len() - 1;
        let footer = if let Some(s) = &self.status {
            Line::from(Span::styled(s.clone(), theme::error()))
        } else {
            Line::from(Span::styled(
                "tab: next · shift-tab: prev · enter: submit · esc: cancel",
                theme::status_bar(),
            ))
        };
        frame.render_widget(Paragraph::new(footer), rows[footer_idx]);
    }
}

fn render_field(frame: &mut Frame, area: Rect, field: &Field, focused: bool) {
    let label_style = if focused {
        Style::default().fg(ratatui::style::Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let value_style = if focused {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };

    let (label, value, suffix): (&str, String, String) = match field {
        Field::Text(t) => (t.label, format_text_with_cursor(t, focused), String::new()),
        Field::Choice(c) => (
            c.label,
            c.value().to_string(),
            format!("  [{}/{}]", c.selected + 1, c.options.len()),
        ),
    };

    let line = Line::from(vec![
        Span::styled(format!("{:>16}: ", label), label_style),
        Span::styled(value, value_style),
        Span::styled(suffix, theme::status_bar()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn format_text_with_cursor(t: &TextField, focused: bool) -> String {
    if !focused {
        return t.value.clone();
    }
    // Insert a visible block cursor at the current position.
    let mut out = String::new();
    let chars: Vec<char> = t.value.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i == t.cursor { out.push('│'); }
        out.push(*c);
    }
    if t.cursor >= chars.len() {
        out.push('│');
    }
    out
}

fn centered_rect(area: Rect, width_pct: u16, height_cells: u16) -> Rect {
    let h = height_cells.min(area.height.saturating_sub(2));
    let w = (area.width as u32 * width_pct as u32 / 100) as u16;
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height - h) / 2),
            Constraint::Length(h),
            Constraint::Min(0),
        ])
        .split(area);
    let h_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width - w) / 2),
            Constraint::Length(w),
            Constraint::Min(0),
        ])
        .split(v[1]);
    h_layout[1]
}

