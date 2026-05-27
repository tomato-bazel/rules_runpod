//! Templates tab — list + delete + detail pane.

use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};

use tui_kit::ui::modal::render_confirm;
use tui_kit::ui::table::{Row, SelectableTable};
use tui_kit::view::{DetailPane, Outcome, View};

pub struct TemplatesView {
    table: SelectableTable<TemplateRow>,
    confirm: Option<String>,
    pending_delete: Option<String>,
    show_detail: bool,
}

impl TemplatesView {
    pub fn new() -> Self {
        Self {
            table: SelectableTable::new(vec!["ID", "NAME", "IMAGE", "DISK", "KIND"]),
            confirm: None,
            pending_delete: None,
            show_detail: false,
        }
    }

    fn selected_id(&self) -> Option<String> {
        self.table.selected().and_then(|r| r.raw.id.clone())
    }
}

#[async_trait]
impl View<runpod::Client> for TemplatesView {
    fn title(&self) -> &str { "Templates" }

    fn count(&self) -> Option<usize> { Some(self.table.rows().len()) }

    fn help(&self) -> String {
        "enter detail · d delete · r refresh · tab switch · q quit".into()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.table.render(frame, area);
        if let Some(prompt) = &self.confirm {
            render_confirm(frame, area, prompt);
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Outcome {
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
            KeyCode::Char('d') => {
                if let Some(id) = self.selected_id() {
                    self.confirm = Some(format!("Delete template {id}?"));
                    self.pending_delete = Some(id);
                }
            }
            _ => {}
        }
        Outcome::Continue
    }

    async fn refresh(&mut self, client: &runpod::Client) -> Result<()> {
        let templates = client.list_templates().await?;
        let rows: Vec<TemplateRow> = templates.into_iter().map(|raw| TemplateRow { raw }).collect();
        self.table.set_rows(rows);
        Ok(())
    }

    async fn run_action(&mut self, client: &runpod::Client) -> Result<()> {
        let Some(id) = self.pending_delete.take() else { return Ok(()); };
        client.delete_template(&id).await?;
        Ok(())
    }

    fn detail(&self) -> Option<DetailPane> {
        if !self.show_detail { return None; }
        let row = self.table.selected()?;
        let t = &row.raw;
        let pairs = vec![
            ("id".into(), t.id.clone().unwrap_or_default()),
            ("name".into(), t.name.clone().unwrap_or_default()),
            ("image".into(), t.image_name.clone().unwrap_or_default()),
            (
                "container disk".into(),
                t.container_disk_in_gb
                    .map(|n| format!("{n} GB"))
                    .unwrap_or_default(),
            ),
            ("ports".into(), t.ports.join(", ")),
            (
                "entrypoint".into(),
                t.docker_entrypoint.join(" "),
            ),
            (
                "cmd".into(),
                t.docker_start_cmd.join(" "),
            ),
            (
                "is public".into(),
                t.is_public.map(|b| b.to_string()).unwrap_or_default(),
            ),
            (
                "is runpod".into(),
                t.is_runpod.map(|b| b.to_string()).unwrap_or_default(),
            ),
            (
                "is serverless".into(),
                t.is_serverless.map(|b| b.to_string()).unwrap_or_default(),
            ),
            ("category".into(), t.category.clone().unwrap_or_default()),
            (
                "env".into(),
                {
                    let mut keys: Vec<_> = t.env.keys().collect();
                    keys.sort();
                    keys.iter()
                        .map(|k| format!("{k}={}", t.env[k.as_str()]))
                        .collect::<Vec<_>>()
                        .join("\n")
                },
            ),
        ];
        Some(DetailPane {
            title: t.name.clone().unwrap_or("template".into()),
            pairs,
        })
    }
}

pub struct TemplateRow {
    raw: runpod::types::Template,
}

impl Row for TemplateRow {
    fn cells(&self) -> Vec<String> {
        let t = &self.raw;
        let kind = match (t.is_serverless, t.is_runpod, t.is_public) {
            (Some(true), _, _) => "serverless",
            (_, Some(true), _) => "runpod",
            (_, _, Some(true)) => "public",
            _ => "private",
        };
        vec![
            t.id.clone().unwrap_or_default(),
            t.name.clone().unwrap_or_default(),
            t.image_name.clone().unwrap_or_default(),
            t.container_disk_in_gb
                .map(|n| format!("{n} GB"))
                .unwrap_or_default(),
            kind.into(),
        ]
    }
}
