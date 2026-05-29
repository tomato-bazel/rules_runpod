//! Network volume subcommands. Volumes persist across pod restarts
//! and across serverless workers — useful for caching large model
//! weights (so a serverless vLLM worker mounts the model instead of
//! re-downloading tens of GB on every cold start) and for storing
//! datasets + checkpoints across training runs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use aws_sdk_s3::primitives::ByteStream;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List network volumes attached to your account.
    List,
    /// Show one volume as JSON.
    Get { id: String },
    /// Upload a local file or directory to a volume via RunPod's
    /// S3-compatible API — the fast data path that replaces
    /// rsync-over-SSH for bulk training data / model weights. Needs
    /// `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` (RunPod S3 API
    /// keys, distinct from `RUNPOD_API_KEY`) in the env; the S3
    /// endpoint + region are derived from the volume's data center.
    Sync(SyncArgs),
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

#[derive(clap::Args, Debug)]
pub struct SyncArgs {
    /// Local file or directory to upload.
    pub local: PathBuf,
    /// Target network volume id.
    pub id: String,
    /// Remote key prefix within the volume (e.g. `datasets/parser`).
    #[arg(long, default_value = "")]
    pub prefix: String,
}

pub async fn dispatch(client: &runpod::Client, cmd: Command) -> Result<()> {
    match cmd {
        Command::List => list(client).await,
        Command::Get { id } => {
            let v = client.get_network_volume(&id).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
            Ok(())
        }
        Command::Sync(args) => sync(client, args).await,
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

// ─── S3-compatible volume access ────────────────────────────────────

/// Build an S3 client pointed at the volume's data center endpoint
/// (`https://s3api-<dc>.runpod.io`, path-style addressing). Credentials
/// come from the standard AWS env vars (RunPod S3 API keys).
async fn s3_for_volume(
    client: &runpod::Client,
    volume_id: &str,
) -> Result<(aws_sdk_s3::Client, String)> {
    let v = client.get_network_volume(volume_id).await?;
    let dc = v
        .data_center_id
        .context("volume has no data_center_id; cannot derive its S3 endpoint")?;
    let region = dc.to_lowercase(); // EU-RO-1 → eu-ro-1
    let endpoint = format!("https://s3api-{region}.runpod.io");
    let base = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region.clone()))
        .endpoint_url(&endpoint)
        .load()
        .await;
    // RunPod's S3 gateway needs path-style addressing (bucket in the path,
    // not a vhost subdomain).
    let conf = aws_sdk_s3::config::Builder::from(&base)
        .force_path_style(true)
        .build();
    Ok((aws_sdk_s3::Client::from_conf(conf), region))
}

/// Collect `(absolute_path, key_relative_to_root)` for every file under
/// `cur`. A single file uploads under its own name; a directory uploads
/// its contents (keys relative to the directory).
fn collect_files(root: &Path, cur: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<()> {
    let meta = std::fs::metadata(cur).with_context(|| format!("stat {cur:?}"))?;
    if meta.is_file() {
        let rel = if root == cur {
            cur.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .context("file has no name")?
        } else {
            cur.strip_prefix(root)
                .unwrap_or(cur)
                .to_string_lossy()
                .replace('\\', "/")
        };
        out.push((cur.to_path_buf(), rel));
        return Ok(());
    }
    let mut entries: Vec<PathBuf> =
        std::fs::read_dir(cur)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort(); // deterministic upload order
    for p in entries {
        collect_files(root, &p, out)?;
    }
    Ok(())
}

async fn sync(client: &runpod::Client, args: SyncArgs) -> Result<()> {
    let (s3, region) = s3_for_volume(client, &args.id).await?;
    let prefix = args.prefix.trim_matches('/').to_string();
    let mut files = Vec::new();
    collect_files(&args.local, &args.local, &mut files)?;
    eprintln!(
        "[runpod-cli] uploading {} file(s) → s3://{}/{} ({})",
        files.len(),
        args.id,
        prefix,
        region
    );
    let mut bytes_total: u64 = 0;
    for (path, rel) in &files {
        let key = if prefix.is_empty() { rel.clone() } else { format!("{prefix}/{rel}") };
        let body = ByteStream::from_path(path)
            .await
            .with_context(|| format!("reading {path:?}"))?;
        bytes_total += std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        s3.put_object()
            .bucket(&args.id)
            .key(&key)
            .body(body)
            .send()
            .await
            .with_context(|| format!("uploading {key}"))?;
        eprintln!("  → {key}");
    }
    println!(
        "uploaded {} file(s) ({:.1} MB) to volume {} (prefix {:?})",
        files.len(),
        bytes_total as f64 / 1.0e6,
        args.id,
        prefix
    );
    Ok(())
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
