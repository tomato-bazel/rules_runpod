//! CLI surface — top-level subcommand dispatch.
//!
//! Each resource (pods, templates, network volumes) lives in its own
//! module that owns the clap subcommand enum + the per-verb handlers.
//! `train` and the active-pod operations (ssh / exec / resume / pull /
//! status / down) live in `train.rs` because they all read the pod-id
//! out of `.runpod/state.toml` and form one user-facing flow.

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::config::Config;
use crate::{endpoint, pod, template, train, volume};

#[derive(Parser, Debug)]
#[command(name = "runpod-cli", about = "Manage RunPod GPU pods and training runs.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Provision a pod, sync the workdir, run setup + run, stream logs.
    Train(train::TrainArgs),
    /// Open an interactive ssh session to the active pod.
    Ssh,
    /// Run a one-off command on the active pod.
    Exec { cmd: Vec<String> },
    /// Re-stream the train command (re-runs `run:` from the manifest).
    Resume,
    /// Pull a remote path back to the local repo via rsync.
    Pull {
        remote: String,
        #[arg(default_value = ".")]
        local: std::path::PathBuf,
    },
    /// Show the active pod's status (id, ssh host:port).
    Status,
    /// Terminate the active pod and clear local state.
    Down,
    /// Pod operations (list / get / start / stop / restart / reset / delete).
    #[command(subcommand)]
    Pod(pod::Command),
    /// Template operations (list / get / delete).
    #[command(subcommand)]
    Template(template::Command),
    /// Network volume operations (list / get / delete).
    #[command(subcommand)]
    Volume(volume::Command),
    /// Serverless endpoint operations (list / get / create / update / delete).
    /// Serverless endpoints are pay-per-request, autoscaling containers
    /// (e.g. for hosting vLLM-served LLMs with an OpenAI-compatible API).
    #[command(subcommand)]
    Endpoint(endpoint::Command),
}

pub async fn run(cli: Cli) -> Result<()> {
    let config = Config::load()?;
    let client = runpod::Client::new(config.api_key.clone())?;

    match cli.command {
        Command::Train(args) => train::cmd_train(&client, &config, args).await,
        Command::Ssh => train::cmd_ssh(&config).await,
        Command::Exec { cmd } => train::cmd_exec(&config, cmd).await,
        Command::Resume => train::cmd_resume(&config).await,
        Command::Pull { remote, local } => train::cmd_pull(&config, &remote, &local).await,
        Command::Status => train::cmd_status(&client, &config).await,
        Command::Down => train::cmd_down(&client, &config).await,
        Command::Pod(c) => pod::dispatch(&client, c).await,
        Command::Template(c) => template::dispatch(&client, c).await,
        Command::Volume(c) => volume::dispatch(&client, c).await,
        Command::Endpoint(c) => endpoint::dispatch(&client, c).await,
    }
}
