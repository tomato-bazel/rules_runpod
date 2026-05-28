//! Network volume subcommands. Volumes persist across pod restarts
//! and across serverless workers — useful for caching large model
//! weights (so a serverless vLLM worker mounts the model instead of
//! re-downloading tens of GB on every cold start) and for storing
//! datasets + checkpoints across training runs.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List network volumes attached to your account.
    List,
    /// Show one volume as JSON.
    Get { id: String },
    /// Create a new network volume.
    Create(CreateArgs),
    /// Create from a JSON file (NetworkVolumeCreateInput shape).
    /// Used by the Bazel `runpod_network_volume` rule's `.create`
    /// runner. If a volume with the same name already exists in the
    /// same data center, prints that volume's id instead of creating
    /// a duplicate (idempotent create — safe to re-run).
    CreateFromSpec { path: PathBuf },
    /// Delete a volume (irreversible — data is wiped).
    Delete { id: String },
}

#[derive(clap::Args, Debug)]
pub struct CreateArgs {
    #[arg(long)]
    pub name: String,
    /// Size in GB.
    #[arg(long)]
    pub size: i64,
    /// RunPod data center id, e.g. EU-RO-1, US-CA-1.
    #[arg(long)]
    pub data_center: String,
}

pub async fn dispatch(client: &runpod::Client, cmd: Command) -> Result<()> {
    match cmd {
        Command::List => list(client).await,
        Command::Get { id } => {
            let v = client.get_network_volume(&id).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
            Ok(())
        }
        Command::Create(args) => {
            let body = runpod::types::NetworkVolumeCreateInput {
                name: args.name,
                size: args.size,
                data_center_id: args.data_center,
            };
            let v = client.create_network_volume(&body).await?;
            println!("created {}", v.id.unwrap_or_default());
            Ok(())
        }
        Command::CreateFromSpec { path } => create_from_spec(client, &path).await,
        Command::Delete { id } => {
            client.delete_network_volume(&id).await?;
            println!("deleted {id}");
            Ok(())
        }
    }
}

async fn create_from_spec(client: &runpod::Client, path: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read volume spec from {path:?}"))?;
    let body: runpod::types::NetworkVolumeCreateInput = serde_json::from_str(&text)
        .with_context(|| format!("parse {path:?} as NetworkVolumeCreateInput"))?;
    // Idempotence: if a same-name volume already exists in the same
    // data center, reuse it. Volumes are persistent, named resources
    // meant to be created once and referenced by many endpoints/pods.
    let existing = client.list_network_volumes().await?;
    if let Some(v) = existing.iter().find(|v| {
        v.name.as_deref() == Some(body.name.as_str())
            && v.data_center_id.as_deref() == Some(body.data_center_id.as_str())
    }) {
        let id = v.id.clone().unwrap_or_default();
        eprintln!(
            "volume {:?} already exists in {} — reusing",
            body.name, body.data_center_id
        );
        println!("{id}");
        return Ok(());
    }
    let v = client.create_network_volume(&body).await?;
    println!("{}", v.id.unwrap_or_default());
    Ok(())
}

async fn list(client: &runpod::Client) -> Result<()> {
    let vols = client.list_network_volumes().await?;
    if vols.is_empty() {
        println!("(no network volumes)");
        return Ok(());
    }
    let rows: Vec<[String; 4]> = vols
        .iter()
        .map(|v| {
            let id = v.id.clone().unwrap_or_else(|| "?".into());
            let name = v.name.clone().unwrap_or_default();
            let size = v
                .size
                .map(|n| format!("{n} GB"))
                .unwrap_or_default();
            let dc = v.data_center_id.clone().unwrap_or_default();
            [id, name, size, dc]
        })
        .collect();
    let headers = ["ID", "NAME", "SIZE", "DATA_CENTER"];
    crate::ui_text::print_table(&headers, &rows);
    Ok(())
}
