//! Pod CRUD subcommands: list / get / start / stop / restart / reset /
//! delete. Operates on pod ids supplied on the command line — the
//! train flow's active-pod state is intentionally not involved here.

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List all pods on the account.
    List,
    /// Show full details for one pod.
    Get { id: String },
    /// Create a new pod. Required: --image, --gpu-type. Other fields
    /// have sensible training defaults.
    Create(CreateArgs),
    /// Start a stopped pod.
    Start { id: String },
    /// Stop a running pod (its volume persists; pod can be started again).
    Stop { id: String },
    /// Restart a running pod in-place.
    Restart { id: String },
    /// Reset a pod (full power-cycle, slower than restart).
    Reset { id: String },
    /// Permanently delete a pod (irreversible).
    Delete { id: String },
}

#[derive(clap::Args, Debug)]
pub struct CreateArgs {
    #[arg(long, default_value = "training")]
    pub name: String,
    #[arg(long)]
    pub image: String,
    #[arg(long, default_value = "NVIDIA A100 80GB PCIe")]
    pub gpu_type: String,
    #[arg(long, default_value_t = 1)]
    pub gpu_count: u32,
    #[arg(long, default_value_t = 50)]
    pub container_disk_gb: i64,
    #[arg(long, default_value_t = 200)]
    pub volume_gb: i64,
    /// COMMUNITY or SECURE.
    #[arg(long, default_value = "COMMUNITY")]
    pub cloud: String,
    /// Comma-separated `<port>/<proto>` list.
    #[arg(long, default_value = "22/tcp,8080/http")]
    pub ports: String,
    /// SSH public key to inject as PUBLIC_KEY env var. Defaults to
    /// reading ~/.ssh/id_ed25519.pub.
    #[arg(long)]
    pub ssh_pub_key: Option<String>,
    /// Attach an existing network volume by id.
    #[arg(long)]
    pub network_volume_id: Option<String>,
}

pub async fn dispatch(client: &runpod::Client, cmd: Command) -> Result<()> {
    match cmd {
        Command::List => list(client).await,
        Command::Get { id } => get(client, &id).await,
        Command::Create(args) => create(client, args).await,
        Command::Start { id } => {
            client.start_pod(&id).await?;
            println!("started {id}");
            Ok(())
        }
        Command::Stop { id } => {
            client.stop_pod(&id).await?;
            println!("stopped {id}");
            Ok(())
        }
        Command::Restart { id } => {
            client.restart_pod(&id).await?;
            println!("restarted {id}");
            Ok(())
        }
        Command::Reset { id } => {
            client.reset_pod(&id).await?;
            println!("reset {id}");
            Ok(())
        }
        Command::Delete { id } => {
            client.terminate_pod(&id).await?;
            println!("deleted {id}");
            Ok(())
        }
    }
}

async fn list(client: &runpod::Client) -> Result<()> {
    let pods = client.list_pods().await?;
    if pods.is_empty() {
        println!("(no pods)");
        return Ok(());
    }
    let rows: Vec<[String; 6]> = pods
        .iter()
        .map(|p| {
            let id = p.id.clone().unwrap_or_else(|| "?".into());
            let status = p
                .desired_status
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "?".into());
            let name = p
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_default();
            let gpu = p
                .gpu
                .as_ref()
                .and_then(|g| g.display_name.as_ref())
                .map(|n| n.to_string())
                .unwrap_or_default();
            let image = p.image.clone().unwrap_or_default();
            let started = p.last_started_at.clone().unwrap_or_default();
            [id, status, name, gpu, image, started]
        })
        .collect();
    let headers = ["ID", "STATUS", "NAME", "GPU", "IMAGE", "STARTED"];
    crate::ui_text::print_table(&headers, &rows);
    Ok(())
}

async fn get(client: &runpod::Client, id: &str) -> Result<()> {
    let pod = client.get_pod_full(id).await?;
    println!("{}", serde_json::to_string_pretty(&pod)?);
    Ok(())
}

async fn create(client: &runpod::Client, args: CreateArgs) -> Result<()> {
    use runpod::types::*;

    let cloud = match args.cloud.as_str() {
        "SECURE" => PodCreateInputCloudType::Secure,
        _ => PodCreateInputCloudType::Community,
    };
    let name = PodCreateInputName::try_from(args.name.as_str())
        .map_err(|e| anyhow::anyhow!("invalid name {:?}: {e}", args.name))?;
    let gpu = args
        .gpu_type
        .as_str()
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid gpu type {:?}: {e}", args.gpu_type))?;

    let ssh_key = match args.ssh_pub_key {
        Some(k) => Some(k),
        None => read_default_pubkey(),
    };
    let mut env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(k) = ssh_key {
        let trimmed = k.trim().to_string();
        if !trimmed.is_empty() {
            env.insert("PUBLIC_KEY".into(), trimmed);
        }
    }

    let ports: Vec<String> = args
        .ports
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let body = PodCreateInput {
        name,
        image_name: Some(args.image),
        container_disk_in_gb: Some(args.container_disk_gb),
        volume_in_gb: Some(args.volume_gb),
        cloud_type: cloud,
        env,
        ports,
        gpu_type_ids: vec![gpu],
        gpu_count: std::num::NonZeroU64::new(args.gpu_count as u64)
            .ok_or_else(|| anyhow::anyhow!("gpu_count must be > 0"))?,
        network_volume_id: args.network_volume_id,
        ..Default::default()
    };
    let pod = client
        .inner()
        .create_pod(&body)
        .await
        .map_err(|e| anyhow::anyhow!("create_pod: {e}"))?
        .into_inner();
    println!("created {}", pod.id.unwrap_or_default());
    Ok(())
}

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
