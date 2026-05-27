//! `SelectableTable` — a width-aware scrollable table with a current
//! selection. The view supplies the row data via the `Row` trait; this
//! module owns the selection state, the scroll offset, and the keymap
//! conventions (↑/↓/k/j navigate, PgUp/PgDn jump, Home/End extremes).

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, Row as TuiRow, Table, TableState},
};

use crate::theme;

/// Each tab's row type implements this so the table widget can fetch
/// cells without owning the underlying data shape.
pub trait Row {
    fn cells(&self) -> Vec<String>;
}

pub struct SelectableTable<R: Row> {
    rows: Vec<R>,
    state: TableState,
    pub headers: Vec<&'static str>,
}

impl<R: Row> SelectableTable<R> {
    pub fn new(headers: Vec<&'static str>) -> Self {
        let mut state = TableState::default();
        state.select(Some(0));
        Self { rows: Vec::new(), state, headers }
    }

    pub fn set_rows(&mut self, rows: Vec<R>) {
        self.rows = rows;
        // Clamp selection.
        if let Some(sel) = self.state.selected() {
            if sel >= self.rows.len() {
                self.state.select(if self.rows.is_empty() { None } else { Some(self.rows.len() - 1) });
            }
        } else if !self.rows.is_empty() {
            self.state.select(Some(0));
        }
    }

    pub fn selected(&self) -> Option<&R> {
        self.state.selected().and_then(|i| self.rows.get(i))
    }

    pub fn rows(&self) -> &[R] {
        &self.rows
    }

    pub fn select_up(&mut self) {
        let n = self.rows.len();
        if n == 0 { return; }
        let i = self.state.selected().unwrap_or(0);
        self.state.select(Some(if i == 0 { n - 1 } else { i - 1 }));
    }

    pub fn select_down(&mut self) {
        let n = self.rows.len();
        if n == 0 { return; }
        let i = self.state.selected().unwrap_or(0);
        self.state.select(Some((i + 1) % n));
    }

    pub fn select_first(&mut self) {
        if !self.rows.is_empty() { self.state.select(Some(0)); }
    }

    pub fn select_last(&mut self) {
        if !self.rows.is_empty() { self.state.select(Some(self.rows.len() - 1)); }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let header = TuiRow::new(
            self.headers.iter().map(|h| Cell::from(*h)).collect::<Vec<_>>(),
        )
        .style(theme::header_row());

        let widths: Vec<Constraint> = self
            .headers
            .iter()
            .map(|_| Constraint::Fill(1))
            .collect();

        let body: Vec<TuiRow> = self
            .rows
            .iter()
            .map(|r| {
                TuiRow::new(
                    r.cells()
                        .into_iter()
                        .map(Cell::from)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();

        let table = Table::new(body, widths)
            .header(header)
            .row_highlight_style(theme::selected_row())
            .style(Style::default())
            .block(Block::default().borders(Borders::ALL));
        frame.render_stateful_widget(table, area, &mut self.state);
    }
}
