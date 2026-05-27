//! Template subcommands. Templates are reusable image+config bundles
//! you point a pod at. We expose read+delete here; create needs more
//! fields than fit on a CLI line and is better done from the TUI.

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List templates visible to your account (including RunPod's
    /// public templates and any saved by you or your team).
    List,
    /// Show one template as JSON.
    Get { id: String },
    /// Delete a template you own.
    Delete { id: String },
}

pub async fn dispatch(client: &runpod::Client, cmd: Command) -> Result<()> {
    match cmd {
        Command::List => list(client).await,
        Command::Get { id } => {
            let t = client.get_template(&id).await?;
            println!("{}", serde_json::to_string_pretty(&t)?);
            Ok(())
        }
        Command::Delete { id } => {
            client.delete_template(&id).await?;
            println!("deleted {id}");
            Ok(())
        }
    }
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
