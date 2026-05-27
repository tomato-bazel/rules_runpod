//! Network volume subcommands. Volumes persist across pod restarts —
//! useful for storing the prime-shards dataset + checkpoints across
//! training runs without rebuilding them each time.

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List network volumes attached to your account.
    List,
    /// Show one volume as JSON.
    Get { id: String },
    /// Create a new network volume.
    Create(CreateArgs),
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
        Command::Delete { id } => {
            client.delete_network_volume(&id).await?;
            println!("deleted {id}");
            Ok(())
        }
    }
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
