//! Train flow + active-pod operations.
//!
//! Everything in this module operates on the pod id recorded in
//! `.runpod/state.toml`. `cmd_train` is the headline; the other
//! commands (ssh / exec / resume / pull / status / down) reach for
//! the same recorded pod so a long-running session is interactive.

use anyhow::{Context, Result, bail};
use clap::Parser;
use std::path::PathBuf;

use crate::config::Config;
use crate::manifest::Manifest;
use crate::ssh;
use crate::state::State;

#[derive(Parser, Debug)]
pub struct TrainArgs {
    /// Path to the runpod.toml manifest describing the job.
    #[arg(short = 'f', long, default_value = "runpod.toml")]
    pub manifest: PathBuf,

    /// Override the GPU type. Defaults to the manifest's value.
    #[arg(long)]
    pub gpu: Option<String>,

    /// Override the pod name shown in the RunPod console.
    #[arg(long)]
    pub name: Option<String>,

    /// Skip the optional `outputs` rsync-back at the end of the run.
    #[arg(long)]
    pub no_pull: bool,

    /// Terminate the pod automatically when training exits successfully.
    #[arg(long)]
    pub down_on_success: bool,

    /// Terminate the pod automatically when setup/run exits non-zero.
    /// Pair with `--down-on-success` for fully-ephemeral one-shot
    /// training jobs that don't leave orphan pods on iteration.
    #[arg(long)]
    pub down_on_failure: bool,

    /// Re-use an existing pod recorded in .runpod/state.toml instead of
    /// provisioning a fresh one.
    #[arg(long)]
    pub reuse: bool,
}

pub async fn cmd_train(
    client: &runpod::Client,
    config: &Config,
    args: TrainArgs,
) -> Result<()> {
    let mut manifest = Manifest::load(&args.manifest)
        .with_context(|| format!("loading manifest {}", args.manifest.display()))?;
    let pub_key = ssh::read_or_generate_pubkey(&config.ssh_key_path()?)?;

    // Forward selected env vars from the local environment (already
    // populated by .env loading in main) into the pod's env dict.
    for name in &manifest.forward_envs {
        match std::env::var(name) {
            Ok(v) if !v.is_empty() => {
                manifest.envs.insert(name.clone(), v);
            }
            _ => eprintln!("[runpod-cli] WARN: forward_envs[{name}] not set locally — skipped"),
        }
    }

    let pod = if args.reuse {
        let st = State::load_or_default()?;
        let pod_id = st
            .pod_id
            .context("no active pod recorded; drop --reuse to provision")?;
        eprintln!("[runpod-cli] reusing pod {pod_id}");
        client.get_pod(&pod_id).await?
    } else {
        let req = runpod::DeployRequest {
            gpu_type: args.gpu.unwrap_or(manifest.resources.gpu_type.clone()),
            gpu_count: 1,
            container_disk_gb: manifest.resources.container_disk_gb,
            volume_gb: manifest.resources.volume_gb,
            image: manifest.resources.image.clone(),
            ports: "22/tcp,8080/http".into(),
            name: args.name.unwrap_or_else(|| manifest.name.clone()),
            public_key: pub_key,
            cloud_type: manifest.resources.cloud_type.clone(),
            env: manifest.envs.clone(),
            network_volume_id: manifest.resources.network_volume_id.clone(),
            data_center_id: manifest.resources.data_center_id.clone(),
        };
        eprintln!(
            "[runpod-cli] deploying {} on {} ({})",
            req.name, req.gpu_type, req.cloud_type
        );
        let pod = client.deploy_on_demand(&req).await?;
        eprintln!("[runpod-cli] pod {} created — waiting for SSH", pod.id);
        let ready = client
            .wait_for_ssh(&pod.id, manifest.resources.wait_timeout_secs)
            .await?;
        State::save(&State { pod_id: Some(ready.id.clone()) })?;
        ready
    };

    let endpoint = pod
        .ssh_endpoint()
        .context("pod has no ssh endpoint despite passing wait_for_ssh")?;
    eprintln!(
        "[runpod-cli] ssh ready: {}@{}:{}",
        endpoint.user, endpoint.host, endpoint.port
    );

    // Ctrl-C → terminate pod. The handler runs on a separate thread
    // so it can spin up its own tokio runtime to call the API.
    let pod_id_for_ctrlc = pod.id.clone();
    let api_key_for_ctrlc = config.api_key.clone();
    ctrlc::set_handler(move || {
        eprintln!("\n[runpod-cli] caught Ctrl-C — terminating pod {pod_id_for_ctrlc}");
        let pod_id = pod_id_for_ctrlc.clone();
        let api_key = api_key_for_ctrlc.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime for ctrl-c handler");
            rt.block_on(async {
                if let Ok(c) = runpod::Client::new(api_key) {
                    let _ = c.terminate_pod(&pod_id).await;
                }
            });
            std::process::exit(130);
        });
    })?;

    ssh::rsync_up(&endpoint, &manifest.workdir, "/root/workdir/", &manifest.skip_patterns)?;

    // Capture failures so we can still pull partial outputs + tear
    // down the pod under `--down-on-failure`. Without this, a non-zero
    // exit from setup/run propagates immediately and orphans the pod.
    let mut script_result: Result<()> = Ok(());
    // Setup always runs in the foreground — it's short (pip install
    // + model prefetch) and a drop there is cheap to retry. The
    // `run` script is the long pole; in detached mode it survives
    // SSH drops.
    script_result = script_result
        .and_then(|_| ssh::run_remote_script_env(&endpoint, "setup", &manifest.setup, &manifest.envs));
    if script_result.is_ok() {
        if manifest.detached {
            // Launch detached, then poll for the exit sentinel. A
            // dropped SSH connection during the poll is non-fatal —
            // the next tick reconnects; the training keeps running
            // on the pod regardless.
            script_result = ssh::run_remote_script_detached(
                &endpoint, "run", &manifest.run, &manifest.envs,
            )
            .and_then(|_| {
                let code = ssh::poll_remote_until_done(&endpoint, manifest.poll_secs)?;
                if code == 0 {
                    Ok(())
                } else {
                    anyhow::bail!("detached `run` exited with code {code}")
                }
            });
        } else {
            script_result =
                ssh::run_remote_script_env(&endpoint, "run", &manifest.run, &manifest.envs);
        }
    }

    // Pull any outputs that did get produced, even on failure —
    // checkpointers often write partial state to outputs/ before
    // the recipe errors.
    if !args.no_pull {
        for path in &manifest.outputs {
            let remote = format!("/root/workdir/{path}");
            eprintln!("[runpod-cli] pulling {remote} -> {path}");
            let _ = ssh::rsync_down(&endpoint, &remote, &PathBuf::from(path));
        }
    }

    let succeeded = script_result.is_ok();
    let should_terminate = (succeeded && args.down_on_success)
        || (!succeeded && args.down_on_failure);

    if should_terminate {
        if succeeded {
            eprintln!("[runpod-cli] training succeeded — terminating pod {}", pod.id);
        } else {
            eprintln!(
                "[runpod-cli] training failed — terminating pod {} (--down-on-failure)",
                pod.id
            );
        }
        client.terminate_pod(&pod.id).await?;
        State::clear()?;
    } else {
        eprintln!(
            "[runpod-cli] pod {} left running. terminate with: runpod-cli down",
            pod.id
        );
    }

    script_result
}

pub async fn cmd_ssh(config: &Config) -> Result<()> {
    let pod = active_pod(config).await?;
    let endpoint = pod.ssh_endpoint().context("pod has no ssh endpoint")?;
    ssh::interactive_ssh(&endpoint)
}

pub async fn cmd_exec(config: &Config, cmd: Vec<String>) -> Result<()> {
    if cmd.is_empty() {
        bail!("exec requires a command");
    }
    let pod = active_pod(config).await?;
    let endpoint = pod.ssh_endpoint().context("pod has no ssh endpoint")?;
    ssh::exec_remote(&endpoint, &cmd.join(" "))
}

pub async fn cmd_resume(config: &Config) -> Result<()> {
    let manifest = Manifest::load(&PathBuf::from("runpod.toml"))?;
    let pod = active_pod(config).await?;
    let endpoint = pod.ssh_endpoint().context("pod has no ssh endpoint")?;
    ssh::run_remote_script(&endpoint, "run", &manifest.run)
}

pub async fn cmd_pull(config: &Config, remote: &str, local: &PathBuf) -> Result<()> {
    let pod = active_pod(config).await?;
    let endpoint = pod.ssh_endpoint().context("pod has no ssh endpoint")?;
    ssh::rsync_down(&endpoint, remote, local)
}

pub async fn cmd_status(client: &runpod::Client, _config: &Config) -> Result<()> {
    let st = State::load_or_default()?;
    let Some(pod_id) = st.pod_id else {
        println!("no active pod");
        return Ok(());
    };
    let pod = client.get_pod(&pod_id).await?;
    println!("pod_id : {}", pod.id);
    println!(
        "status : {}",
        pod.desired_status
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "UNKNOWN".into())
    );
    if let Some(endpoint) = pod.ssh_endpoint() {
        println!("ssh    : {}@{}:{}", endpoint.user, endpoint.host, endpoint.port);
    } else if let Some(ip) = pod.public_ip {
        println!("ip     : {ip} (ports not mapped yet)");
    }
    Ok(())
}

pub async fn cmd_down(client: &runpod::Client, _config: &Config) -> Result<()> {
    let st = State::load_or_default()?;
    let Some(pod_id) = st.pod_id else {
        println!("no active pod");
        return Ok(());
    };
    eprintln!("[runpod-cli] terminating pod {pod_id}");
    client.terminate_pod(&pod_id).await?;
    State::clear()?;
    Ok(())
}

async fn active_pod(config: &Config) -> Result<runpod::Pod> {
    let st = State::load_or_default()?;
    let pod_id = st
        .pod_id
        .context("no active pod — run `runpod-cli train` first")?;
    let client = runpod::Client::new(config.api_key.clone())?;
    client.get_pod(&pod_id).await
}
