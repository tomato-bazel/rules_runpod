//! Pods tab — list + lifecycle actions + detail pane.

use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};

use tui_kit::ui::form::{ChoiceField, Field, Form, FormOutcome, TextField};
use tui_kit::ui::modal::render_confirm;
use tui_kit::ui::table::{Row, SelectableTable};
use tui_kit::view::{DetailPane, Outcome, View};

pub struct PodsView {
    table: SelectableTable<PodRow>,
    confirm: Option<Confirm>,
    pending: Option<Action>,
    show_detail: bool,
    pending_ssh: Option<SshTarget>,
    form: Option<Form>,
}

struct SshTarget {
    host: String,
    port: u32,
    user: String,
}

enum Action {
    Start(String),
    Stop(String),
    Restart(String),
    Reset(String),
    Delete(String),
    Create(runpod::types::PodCreateInput),
}

// Indices into the create form's fields. Keep in sync with open_create_form.
const F_NAME: usize = 0;
const F_IMAGE: usize = 1;
const F_GPU: usize = 2;
const F_GPU_COUNT: usize = 3;
const F_DISK: usize = 4;
const F_VOLUME: usize = 5;
const F_CLOUD: usize = 6;
const F_PORTS: usize = 7;
const F_PUBKEY: usize = 8;
const F_NETWORK_VOLUME: usize = 9;

struct Confirm {
    prompt: String,
    action: Action,
}

impl PodsView {
    pub fn new() -> Self {
        Self {
            table: SelectableTable::new(vec![
                "ID", "STATUS", "NAME", "GPU", "IMAGE", "STARTED",
            ]),
            confirm: None,
            pending: None,
            show_detail: false,
            pending_ssh: None,
            form: None,
        }
    }

    fn open_create_form(&mut self) {
        let pubkey = read_default_pubkey().unwrap_or_default();
        let fields = vec![
            Field::Text(TextField::new("name", "training")),
            Field::Text(TextField::new(
                "image",
                "runpod/pytorch:2.4.0-py3.11-cuda12.4.1-devel-ubuntu22.04",
            )),
            Field::Text(TextField::new("gpu type", "NVIDIA A100 80GB PCIe")),
            Field::Text(TextField::new("gpu count", "1")),
            Field::Text(TextField::new("container disk GB", "50")),
            Field::Text(TextField::new("volume GB", "200")),
            Field::Choice(ChoiceField::new(
                "cloud",
                vec!["COMMUNITY", "SECURE"],
                0,
            )),
            Field::Text(TextField::new("ports", "22/tcp,8080/http")),
            Field::Text(TextField::new("ssh pub key", pubkey)),
            Field::Text(TextField::new("network volume id", "")),
        ];
        self.form = Some(Form::new("Create pod", fields));
    }

    fn submit_create_form(&mut self) -> Result<Action, String> {
        let Some(form) = self.form.as_ref() else {
            return Err("no form open".into());
        };
        let gpu_count: u32 = form
            .value(F_GPU_COUNT).unwrap_or("1")
            .parse()
            .map_err(|_| "gpu count must be an integer".to_string())?;
        let disk: i64 = form
            .value(F_DISK).unwrap_or("50")
            .parse()
            .map_err(|_| "container disk must be an integer".to_string())?;
        let vol: i64 = form
            .value(F_VOLUME).unwrap_or("200")
            .parse()
            .map_err(|_| "volume must be an integer".to_string())?;

        let cloud = match form.value(F_CLOUD).unwrap_or("COMMUNITY") {
            "SECURE" => runpod::types::PodCreateInputCloudType::Secure,
            _ => runpod::types::PodCreateInputCloudType::Community,
        };
        let name = runpod::types::PodCreateInputName::try_from(
            form.value(F_NAME).unwrap_or("training"),
        )
        .map_err(|e| format!("invalid name: {e}"))?;
        let gpu = form
            .value(F_GPU).unwrap_or("")
            .parse()
            .map_err(|e| format!("invalid gpu type: {e}"))?;

        let mut env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let pubkey = form.value(F_PUBKEY).unwrap_or("").trim().to_string();
        if !pubkey.is_empty() {
            env.insert("PUBLIC_KEY".into(), pubkey);
        }

        let ports: Vec<String> = form
            .value(F_PORTS).unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let network_volume_id = {
            let s = form.value(F_NETWORK_VOLUME).unwrap_or("").trim();
            if s.is_empty() { None } else { Some(s.to_string()) }
        };

        let body = runpod::types::PodCreateInput {
            name,
            image_name: Some(form.value(F_IMAGE).unwrap_or("").to_string()),
            container_disk_in_gb: Some(disk),
            volume_in_gb: Some(vol),
            cloud_type: cloud,
            env,
            ports,
            gpu_type_ids: vec![gpu],
            gpu_count: std::num::NonZeroU64::new(gpu_count as u64)
                .ok_or("gpu count must be > 0")?,
            network_volume_id,
            ..Default::default()
        };
        Ok(Action::Create(body))
    }

    fn selected_ssh_target(&self) -> Option<SshTarget> {
        let row = self.table.selected()?;
        let ip = row.raw.public_ip.as_ref().filter(|s| !s.is_empty())?;
        let port = *row.raw.port_mappings.as_ref()?.get("22")?;
        if port <= 0 || port > u16::MAX as i64 {
            return None;
        }
        Some(SshTarget {
            host: ip.clone(),
            port: port as u32,
            user: "root".into(),
        })
    }

    fn selected_id(&self) -> Option<String> {
        self.table
            .selected()
            .and_then(|r| r.raw.id.clone())
    }

    fn queue_destructive(&mut self, verb: &str, action: Action) -> Outcome {
        let Some(id) = self.selected_id() else { return Outcome::Continue; };
        self.confirm = Some(Confirm {
            prompt: format!("{verb} pod {id}?"),
            action,
        });
        Outcome::Continue
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(c) = self.confirm.take() {
                    self.pending = Some(c.action);
                    return Outcome::Action;
                }
                Outcome::Continue
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm = None;
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }
}

#[async_trait]
impl View<runpod::Client> for PodsView {
    fn title(&self) -> &str { "Pods" }

    fn count(&self) -> Option<usize> { Some(self.table.rows().len()) }

    fn help(&self) -> String {
        "enter detail · c create · l ssh · s start · S stop · R restart · x reset · d delete · r refresh · q quit"
            .into()
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.table.render(frame, area);
        if let Some(c) = &self.confirm {
            render_confirm(frame, area, &c.prompt);
        }
        if let Some(form) = &self.form {
            form.render(frame, area);
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if self.form.is_some() {
            // Form-mode owns input until submit/cancel.
            let outcome = self.form.as_mut().unwrap().handle_key(key);
            match outcome {
                FormOutcome::None => {}
                FormOutcome::Cancel => self.form = None,
                FormOutcome::Submit => match self.submit_create_form() {
                    Ok(action) => {
                        self.pending = Some(action);
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
            return self.handle_confirm_key(key);
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.table.select_up(),
            KeyCode::Down | KeyCode::Char('j') => self.table.select_down(),
            KeyCode::Home | KeyCode::Char('g') => self.table.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.table.select_last(),
            KeyCode::Enter => self.show_detail = !self.show_detail,
            KeyCode::Esc if self.show_detail => self.show_detail = false,
            KeyCode::Char('r') => return Outcome::Refresh,

            // Drop into ssh against the selected pod (suspends TUI).
            KeyCode::Char('l') => {
                if let Some(target) = self.selected_ssh_target() {
                    self.pending_ssh = Some(target);
                    return Outcome::Subprocess;
                }
            }

            // Open create-pod form.
            KeyCode::Char('c') => {
                self.open_create_form();
            }

            // Non-destructive actions: fire immediately, no confirm.
            KeyCode::Char('s') => {
                if let Some(id) = self.selected_id() {
                    self.pending = Some(Action::Start(id));
                    return Outcome::Action;
                }
            }
            KeyCode::Char('S') => {
                if let Some(id) = self.selected_id() {
                    self.pending = Some(Action::Stop(id));
                    return Outcome::Action;
                }
            }
            KeyCode::Char('R') => {
                if let Some(id) = self.selected_id() {
                    self.pending = Some(Action::Restart(id));
                    return Outcome::Action;
                }
            }

            // Destructive: open confirm modal.
            KeyCode::Char('x') => {
                if let Some(id) = self.selected_id() {
                    return self.queue_destructive("Reset", Action::Reset(id));
                }
            }
            KeyCode::Char('d') => {
                if let Some(id) = self.selected_id() {
                    return self.queue_destructive("Delete", Action::Delete(id));
                }
            }
            _ => {}
        }
        Outcome::Continue
    }

    async fn refresh(&mut self, client: &runpod::Client) -> Result<()> {
        let pods = client.list_pods().await?;
        let rows: Vec<PodRow> = pods.into_iter().map(|raw| PodRow { raw }).collect();
        self.table.set_rows(rows);
        Ok(())
    }

    async fn run_action(&mut self, client: &runpod::Client) -> Result<()> {
        let Some(action) = self.pending.take() else { return Ok(()); };
        match action {
            Action::Start(id) => client.start_pod(&id).await?,
            Action::Stop(id) => client.stop_pod(&id).await?,
            Action::Restart(id) => client.restart_pod(&id).await?,
            Action::Reset(id) => client.reset_pod(&id).await?,
            Action::Delete(id) => client.terminate_pod(&id).await?,
            Action::Create(body) => {
                client
                    .inner()
                    .create_pod(&body)
                    .await
                    .map_err(|e| anyhow::anyhow!("create_pod: {e}"))?;
            }
        }
        Ok(())
    }

    fn take_subprocess(&mut self) -> Option<std::process::Command> {
        let t = self.pending_ssh.take()?;
        let mut cmd = std::process::Command::new("ssh");
        cmd.args([
            "-p",
            &t.port.to_string(),
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "LogLevel=ERROR",
        ]);
        cmd.arg(format!("{}@{}", t.user, t.host));
        Some(cmd)
    }

    fn detail(&self) -> Option<DetailPane> {
        if !self.show_detail { return None; }
        let row = self.table.selected()?;
        let p = &row.raw;
        let mut pairs = vec![
            ("id".into(), opt_str(&p.id)),
            (
                "status".into(),
                p.desired_status
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            ),
            ("name".into(), p.name.as_ref().map(|n| n.to_string()).unwrap_or_default()),
            ("machine".into(), opt_str(&p.machine_id)),
            ("image".into(), opt_str(&p.image)),
            (
                "gpu".into(),
                p.gpu
                    .as_ref()
                    .and_then(|g| g.display_name.clone())
                    .unwrap_or_default(),
            ),
            (
                "gpu count".into(),
                p.gpu
                    .as_ref()
                    .and_then(|g| g.count)
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
            ),
            (
                "vcpus".into(),
                p.vcpu_count.map(|n| n.to_string()).unwrap_or_default(),
            ),
            (
                "ram GB".into(),
                p.memory_in_gb.map(|n| n.to_string()).unwrap_or_default(),
            ),
            (
                "container disk".into(),
                p.container_disk_in_gb.map(|n| format!("{n} GB")).unwrap_or_default(),
            ),
            (
                "volume".into(),
                p.volume_in_gb.map(|n| format!("{n} GB")).unwrap_or_default(),
            ),
            ("volume path".into(), opt_str(&p.volume_mount_path)),
            (
                "public ip".into(),
                p.public_ip.clone().unwrap_or_default(),
            ),
            (
                "port mappings".into(),
                p.port_mappings
                    .as_ref()
                    .map(|m| {
                        let mut entries: Vec<_> = m.iter().collect();
                        entries.sort_by_key(|(k, _)| (*k).clone());
                        entries
                            .iter()
                            .map(|(k, v)| format!("{k}→{v}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default(),
            ),
            (
                "cost / hr".into(),
                p.cost_per_hr.map(|c| format!("${c:.3}")).unwrap_or_default(),
            ),
            ("started".into(), opt_str(&p.last_started_at)),
            ("template".into(), opt_str(&p.template_id)),
            ("network volume".into(), opt_str(&p.network_volume_id())),
        ];
        // Env vars on a single trailing line for readability.
        if !p.env.is_empty() {
            let mut env_keys: Vec<_> = p.env.keys().collect();
            env_keys.sort();
            let joined: Vec<String> = env_keys
                .iter()
                .map(|k| format!("{k}={}", p.env[k.as_str()]))
                .collect();
            pairs.push(("env".into(), joined.join("\n")));
        }
        Some(DetailPane {
            title: p.name.as_ref().map(|n| n.to_string()).unwrap_or("pod".into()),
            pairs,
        })
    }
}

pub struct PodRow {
    raw: runpod::types::Pod,
}

impl Row for PodRow {
    fn cells(&self) -> Vec<String> {
        let p = &self.raw;
        vec![
            p.id.clone().unwrap_or_default(),
            p.desired_status
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            p.name.as_ref().map(|n| n.to_string()).unwrap_or_default(),
            p.gpu
                .as_ref()
                .and_then(|g| g.display_name.clone())
                .unwrap_or_default(),
            p.image.clone().unwrap_or_default(),
            p.last_started_at.clone().unwrap_or_default(),
        ]
    }
}

fn opt_str<S: AsRef<str>>(o: &Option<S>) -> String {
    o.as_ref().map(|s| s.as_ref().to_string()).unwrap_or_default()
}

/// Best-effort read of the user's default ssh public key. Tries
/// ed25519 first, then rsa; returns "" on miss so the form still
/// opens and the user can paste a key by hand.
fn read_default_pubkey() -> Option<String> {
    let home = directories::UserDirs::new()?.home_dir().to_path_buf();
    for name in ["id_ed25519.pub", "id_rsa.pub"] {
        let p = home.join(".ssh").join(name);
        if let Ok(text) = std::fs::read_to_string(&p) {
            return Some(text.trim().to_string());
        }
    }
    None
}

// Helper trait extension since `network_volume` on the generated Pod
// returns a struct rather than just the id. Keeps the detail mapping
// concise.
trait PodExt {
    fn network_volume_id(&self) -> Option<String>;
}

impl PodExt for runpod::types::Pod {
    fn network_volume_id(&self) -> Option<String> {
        self.network_volume.as_ref().and_then(|nv| nv.id.clone())
    }
}
