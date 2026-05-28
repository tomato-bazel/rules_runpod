//! Serverless endpoint subcommands.
//!
//! A Serverless endpoint on RunPod is a pay-per-request, autoscaling
//! container service. Workers spin up on demand, run a request, and
//! scale down when idle. Common use case downstream: host a vLLM /
//! text-generation-inference / llama.cpp server speaking
//! OpenAI-compatible at
//! `https://api.runpod.ai/v2/<endpoint_id>/openai/v1/chat/completions`.
//!
//! Subcommand surface:
//!
//!   runpod endpoint list
//!   runpod endpoint get <id>
//!   runpod endpoint create --template-id <id> --name <name> --gpu <id> \
//!                          [--min-workers N] [--max-workers N] \
//!                          [--idle-timeout-seconds N] \
//!                          [--data-center <id>]...
//!   runpod endpoint create-from-spec <path-to-json>   # full EndpointCreateInput
//!   runpod endpoint update <id> [--min-workers N] [--max-workers N] \
//!                                [--idle-timeout-seconds N]
//!   runpod endpoint delete <id>
//!
//! For the `create` subcommand we expose the most common knobs.
//! Other fields (cuda filters, network volumes, scaler tweaks) go
//! through `create-from-spec` with a hand-authored JSON. Same
//! pattern the Bazel `runpod_serverless_endpoint` macro uses
//! internally.
//!
//! Construction approach: we build the `EndpointCreateInput` /
//! `EndpointUpdateInput` via `serde_json::from_value` rather than
//! struct-literal-with-defaults. The progenitor-generated types
//! don't implement `Default` (because of required fields), and
//! enumerating every optional field every time is noisy. The JSON
//! path respects serde defaults end-to-end so we only spell out
//! the fields the CLI flag surface owns.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Subcommand;
use runpod::types::{EndpointCreateInput, EndpointUpdateInput};
use serde_json::json;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List serverless endpoints visible to your account.
    List,
    /// Show one endpoint as JSON.
    Get { id: String },
    /// Create a new endpoint from explicit CLI args (common knobs).
    Create(CreateArgs),
    /// Create from a JSON file containing a full EndpointCreateInput.
    /// Use for fields not exposed by `create`'s flag surface.
    CreateFromSpec { path: PathBuf },
    /// Update scaling / idle settings on an existing endpoint.
    Update(UpdateArgs),
    /// Delete an endpoint. Workers terminate immediately.
    Delete { id: String },
}

#[derive(clap::Args, Debug)]
pub struct CreateArgs {
    /// Display name for the endpoint.
    #[arg(long)]
    pub name: String,

    /// Existing template id to use. Pre-build the template with
    /// `runpod template create ...` (or via the
    /// `runpod_template` Bazel macro) before referencing it here.
    #[arg(long)]
    pub template_id: String,

    /// GPU type id (e.g. "NVIDIA A100 80GB PCIe"). Repeatable —
    /// list in preference order.
    #[arg(long = "gpu", required = true)]
    pub gpu_type_ids: Vec<String>,

    /// Data center id (e.g. "US-CA-2"). Repeatable. Empty = any.
    #[arg(long = "data-center")]
    pub data_center_ids: Vec<String>,

    /// Minimum workers kept warm. 0 = pure on-demand (cold-starts).
    #[arg(long, default_value_t = 0)]
    pub min_workers: u32,

    /// Maximum concurrent workers.
    #[arg(long, default_value_t = 3)]
    pub max_workers: u32,

    /// Seconds of idleness before a worker is scaled down. Min 1.
    #[arg(long, default_value_t = 5)]
    pub idle_timeout_seconds: u32,

    /// Compute type: gpu (default) or cpu.
    #[arg(long, default_value = "gpu")]
    pub compute_type: String,
}

#[derive(clap::Args, Debug)]
pub struct UpdateArgs {
    /// Endpoint id to update.
    pub id: String,
    #[arg(long)]
    pub min_workers: Option<u32>,
    #[arg(long)]
    pub max_workers: Option<u32>,
    #[arg(long)]
    pub idle_timeout_seconds: Option<u32>,
}

pub async fn dispatch(client: &runpod::Client, cmd: Command) -> Result<()> {
    match cmd {
        Command::List => list(client).await,
        Command::Get { id } => {
            let e = client.get_endpoint(&id).await?;
            println!("{}", serde_json::to_string_pretty(&e)?);
            Ok(())
        }
        Command::Create(args) => create(client, args).await,
        Command::CreateFromSpec { path } => create_from_spec(client, &path).await,
        Command::Update(args) => update(client, args).await,
        Command::Delete { id } => {
            client.delete_endpoint(&id).await?;
            println!("deleted endpoint {id}");
            Ok(())
        }
    }
}

async fn list(client: &runpod::Client) -> Result<()> {
    let endpoints = client.list_endpoints().await?;
    if endpoints.is_empty() {
        println!("(no endpoints)");
        return Ok(());
    }
    let rows: Vec<[String; 4]> = endpoints
        .iter()
        .map(|e| {
            let id = e.id.clone().unwrap_or_default();
            let name = e.name.clone().unwrap_or_default();
            let gpu = e
                .gpu_type_ids
                .iter()
                .next()
                .map(ToString::to_string)
                .unwrap_or_else(|| "-".to_string());
            let workers = format!(
                "{}-{}",
                e.workers_min.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
                e.workers_max.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
            );
            [id, name, workers, gpu]
        })
        .collect();
    let headers = ["ID", "NAME", "WORKERS", "GPU"];
    crate::ui_text::print_table(&headers, &rows);
    Ok(())
}

async fn create(client: &runpod::Client, args: CreateArgs) -> Result<()> {
    let compute_type = match args.compute_type.as_str() {
        "gpu" => "GPU",
        "cpu" => "CPU",
        other => anyhow::bail!("compute_type must be gpu|cpu (got {other:?})"),
    };
    let spec = json!({
        "name": args.name,
        "templateId": args.template_id,
        "gpuTypeIds": args.gpu_type_ids,
        "dataCenterIds": args.data_center_ids,
        "computeType": compute_type,
        "workersMin": args.min_workers,
        "workersMax": args.max_workers,
        "idleTimeout": args.idle_timeout_seconds.max(1),
    });
    let body: EndpointCreateInput = serde_json::from_value(spec)
        .context("build EndpointCreateInput from CLI args")?;
    let endpoint = client.create_endpoint(&body).await?;
    println!("{}", serde_json::to_string_pretty(&endpoint)?);
    Ok(())
}

async fn create_from_spec(client: &runpod::Client, path: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read endpoint spec from {path:?}"))?;
    let body: EndpointCreateInput = serde_json::from_str(&text)
        .with_context(|| format!("parse {path:?} as EndpointCreateInput"))?;
    let endpoint = client.create_endpoint(&body).await?;
    println!("{}", serde_json::to_string_pretty(&endpoint)?);
    Ok(())
}

async fn update(client: &runpod::Client, args: UpdateArgs) -> Result<()> {
    let mut spec = serde_json::Map::new();
    if let Some(n) = args.min_workers {
        spec.insert("workersMin".into(), json!(n));
    }
    if let Some(n) = args.max_workers {
        spec.insert("workersMax".into(), json!(n));
    }
    if let Some(n) = args.idle_timeout_seconds {
        spec.insert("idleTimeout".into(), json!(n.max(1)));
    }
    let body: EndpointUpdateInput = serde_json::from_value(serde_json::Value::Object(spec))
        .context("build EndpointUpdateInput from CLI args")?;
    let endpoint = client.update_endpoint(&args.id, &body).await?;
    println!("{}", serde_json::to_string_pretty(&endpoint)?);
    Ok(())
}
