//! Template subcommands. Templates are reusable image+config bundles
//! you point a pod or serverless endpoint at. CLI surface covers
//! list / get / delete and the two flavors of create:
//!
//!   - `create` for the common case (image + container disk + a few
//!     env vars + optional ports), expressed as flags.
//!   - `create-from-spec` for full `TemplateCreateInput` JSON when
//!     you need fields the flag surface doesn't expose (start command
//!     overrides, custom volumes, etc.). Same pattern as
//!     `endpoint create-from-spec`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Subcommand;
use runpod::types::TemplateCreateInput;
use serde_json::json;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List templates visible to your account (including RunPod's
    /// public templates and any saved by you or your team).
    List,
    /// Show one template as JSON.
    Get { id: String },
    /// Create a template from CLI flags (common case).
    Create(CreateArgs),
    /// Create a template from a JSON file containing a full
    /// TemplateCreateInput. Use for fields the flag surface doesn't
    /// expose.
    CreateFromSpec { path: PathBuf },
    /// Delete a template you own.
    Delete { id: String },
}

#[derive(clap::Args, Debug)]
pub struct CreateArgs {
    /// Display name for the template.
    #[arg(long)]
    pub name: String,
    /// Docker image reference, e.g. `vllm/vllm-openai:latest`.
    #[arg(long)]
    pub image: String,
    /// Container disk in GB. RunPod default = 50.
    #[arg(long, default_value_t = 50)]
    pub container_disk_gb: u32,
    /// Mark the template as serverless-only (cannot be attached to
    /// non-serverless pods).
    #[arg(long)]
    pub serverless: bool,
    /// Environment variable in `KEY=VALUE` form. Repeatable.
    #[arg(long = "env")]
    pub env: Vec<String>,
    /// Ports declaration, e.g. `8000/http`. Repeatable.
    #[arg(long = "port")]
    pub ports: Vec<String>,
    /// Override the image's start command.
    #[arg(long)]
    pub start_cmd: Option<String>,
}

pub async fn dispatch(client: &runpod::Client, cmd: Command) -> Result<()> {
    match cmd {
        Command::List => list(client).await,
        Command::Get { id } => {
            let t = client.get_template(&id).await?;
            println!("{}", serde_json::to_string_pretty(&t)?);
            Ok(())
        }
        Command::Create(args) => create(client, args).await,
        Command::CreateFromSpec { path } => create_from_spec(client, &path).await,
        Command::Delete { id } => {
            client.delete_template(&id).await?;
            println!("deleted {id}");
            Ok(())
        }
    }
}

async fn create(client: &runpod::Client, args: CreateArgs) -> Result<()> {
    // env is a HashMap<String,String> in the proto schema, so we
    // pass it as a JSON object. KEY=VALUE entries are split here.
    let mut env_map = serde_json::Map::new();
    for kv in &args.env {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--env entry {kv:?} must be KEY=VALUE"))?;
        env_map.insert(k.to_string(), json!(v));
    }
    let docker_start_cmd: Vec<String> = args
        .start_cmd
        .as_deref()
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();
    let spec = json!({
        "name": args.name,
        "imageName": args.image,
        "containerDiskInGb": args.container_disk_gb,
        "env": env_map,
        "ports": args.ports,
        "isServerless": args.serverless,
        "dockerStartCmd": docker_start_cmd,
    });
    let body: TemplateCreateInput = serde_json::from_value(spec)
        .context("build TemplateCreateInput from CLI args")?;
    let t = client.create_template(&body).await?;
    println!("{}", serde_json::to_string_pretty(&t)?);
    Ok(())
}

async fn create_from_spec(client: &runpod::Client, path: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read template spec from {path:?}"))?;
    let body: TemplateCreateInput = serde_json::from_str(&text)
        .with_context(|| format!("parse {path:?} as TemplateCreateInput"))?;
    let t = client.create_template(&body).await?;
    println!("{}", serde_json::to_string_pretty(&t)?);
    Ok(())
}

async fn list(client: &runpod::Client) -> Result<()> {
    let templates = client.list_templates().await?;
    if templates.is_empty() {
        println!("(no templates)");
        return Ok(());
    }
    let rows: Vec<[String; 5]> = templates
        .iter()
        .map(|t| {
            let id = t.id.clone().unwrap_or_else(|| "?".into());
            let name = t.name.clone().unwrap_or_default();
            let image = t.image_name.clone().unwrap_or_default();
            let disk = t
                .container_disk_in_gb
                .map(|n| format!("{n} GB"))
                .unwrap_or_default();
            let kind = match (t.is_serverless, t.is_runpod, t.is_public) {
                (Some(true), _, _) => "serverless",
                (_, Some(true), _) => "runpod",
                (_, _, Some(true)) => "public",
                _ => "private",
            };
            [id, name, image, disk, kind.into()]
        })
        .collect();
    let headers = ["ID", "NAME", "IMAGE", "DISK", "KIND"];
    crate::ui_text::print_table(&headers, &rows);
    Ok(())
}
