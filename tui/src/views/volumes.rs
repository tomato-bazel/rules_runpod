//! Network volumes tab — list + delete + detail pane.

use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};

use tui_kit::ui::form::{Field, Form, FormOutcome, TextField};
use tui_kit::ui::modal::render_confirm;
use tui_kit::ui::table::{Row, SelectableTable};
use tui_kit::view::{DetailPane, Outcome, View};

pub struct VolumesView {
    table: SelectableTable<VolumeRow>,
    confirm: Option<String>,
    pending_delete: Option<String>,
    pending_create: Option<runpod::types::NetworkVolumeCreateInput>,
    show_detail: bool,
    form: Option<Form>,
}

// Volume-form field indices.
const VF_NAME: usize = 0;
const VF_SIZE: usize = 1;
const VF_DATACENTER: usize = 2;

impl VolumesView {
    pub fn new() -> Self {
        Self {
            table: SelectableTable::new(vec!["ID", "NAME", "SIZE", "DATA_CENTER"]),
            confirm: None,
            pending_delete: None,
            pending_create: None,
            show_detail: false,
            form: None,
        }
    }

    fn selected_id(&self) -> Option<String> {
        self.table.selected().and_then(|r| r.raw.id.clone())
    }

    fn open_create_form(&mut self) {
        let fields = vec![
            Field::Text(TextField::new("name", "prime-shards")),
            Field::Text(TextField::new("size GB", "500")),
            Field::Text(TextField::new("data center", "EU-RO-1")),
        ];
        self.form = Some(Form::new("Create volume", fields));
    }

    fn submit_create_form(&mut self) -> Result<(), String> {
        let Some(form) = self.form.as_ref() else { return Err("no form".into()); };
        let name = form.value(VF_NAME).unwrap_or("").trim().to_string();
        if name.is_empty() { return Err("name required".into()); }
        let size: i64 = form
            .value(VF_SIZE).unwrap_or("0")
            .parse()
            .map_err(|_| "size must be an integer (GB)".to_string())?;
        if size <= 0 { return Err("size must be > 0".into()); }
        let dc = form.value(VF_DATACENTER).unwrap_or("").trim().to_string();
        if dc.is_empty() { return Err("data center required".into()); }
        self.pending_create = Some(runpod::types::NetworkVolumeCreateInput {
            name,
            size,
            data_center_id: dc,
        });
        Ok(())
    }
}

#[async_trait]
impl View<runpod::Client> for VolumesView {
    fn title(&self) -> &str { "Volumes" }

    fn count(&self) -> Option<usize> { Some(self.table.rows().len()) }

    fn help(&self) -> String {
        "enter detail · c create · d delete · r refresh · tab switch · q quit".into()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.table.render(frame, area);
        if let Some(prompt) = &self.confirm {
            render_confirm(frame, area, prompt);
        }
        if let Some(form) = &self.form {
            form.render(frame, area);
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if self.form.is_some() {
            let outcome = self.form.as_mut().unwrap().handle_key(key);
            match outcome {
                FormOutcome::None => {}
                FormOutcome::Cancel => self.form = None,
                FormOutcome::Submit => match self.submit_create_form() {
                    Ok(()) => {
                        self.form = None;
                        return Outcome::Action;
                    }
                    Err(msg) => {
                        if let Some(f) = self.form.as_mut() {
                            f.status = Some(format!("error: {msg}"));
                        }
                    }
                },
            }
            return Outcome::Continue;
        }
        if self.confirm.is_some() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.confirm = None;
                    return Outcome::Action;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirm = None;
                    self.pending_delete = None;
                }
                _ => {}
            }
            return Outcome::Continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.table.select_up(),
            KeyCode::Down | KeyCode::Char('j') => self.table.select_down(),
            KeyCode::Enter => self.show_detail = !self.show_detail,
            KeyCode::Esc if self.show_detail => self.show_detail = false,
            KeyCode::Char('r') => return Outcome::Refresh,
            KeyCode::Char('c') => self.open_create_form(),
            KeyCode::Char('d') => {
                if let Some(id) = self.selected_id() {
                    self.confirm = Some(format!("Delete volume {id}?"));
                    self.pending_delete = Some(id);
                }
            }
            _ => {}
        }
        Outcome::Continue
    }

    async fn refresh(&mut self, client: &runpod::Client) -> Result<()> {
        let vols = client.list_network_volumes().await?;
        let rows: Vec<VolumeRow> = vols.into_iter().map(|raw| VolumeRow { raw }).collect();
        self.table.set_rows(rows);
        Ok(())
    }

    async fn run_action(&mut self, client: &runpod::Client) -> Result<()> {
        if let Some(body) = self.pending_create.take() {
            client.create_network_volume(&body).await?;
            return Ok(());
        }
        if let Some(id) = self.pending_delete.take() {
            client.delete_network_volume(&id).await?;
        }
        Ok(())
    }

    fn detail(&self) -> Option<DetailPane> {
        if !self.show_detail { return None; }
        let row = self.table.selected()?;
        let v = &row.raw;
        let pairs = vec![
            ("id".into(), v.id.clone().unwrap_or_default()),
            ("name".into(), v.name.clone().unwrap_or_default()),
            ("size".into(), v.size.map(|n| format!("{n} GB")).unwrap_or_default()),
            ("data center".into(), v.data_center_id.clone().unwrap_or_default()),
        ];
        Some(DetailPane {
            title: v.name.clone().unwrap_or("volume".into()),
            pairs,
        })
    }
}

pub struct VolumeRow {
    raw: runpod::types::NetworkVolumesItem,
}

impl Row for VolumeRow {
    fn cells(&self) -> Vec<String> {
        let v = &self.raw;
        vec![
            v.id.clone().unwrap_or_default(),
            v.name.clone().unwrap_or_default(),
            v.size.map(|n| format!("{n} GB")).unwrap_or_default(),
            v.data_center_id.clone().unwrap_or_default(),
        ]
    }
}
