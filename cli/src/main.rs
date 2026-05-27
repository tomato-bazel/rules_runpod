//! runpod-cli — orchestrate a training run on a RunPod GPU pod.
//!
//! Lifecycle for a single `train` invocation:
//!   1. Read the manifest (runpod.toml) describing the job.
//!   2. Read the user's SSH public key.
//!   3. Call RunPod's REST API to deploy an on-demand pod with that
//!      public key injected as PUBLIC_KEY (RunPod templates install it
//!      to /root/.ssh/authorized_keys at startup).
//!   4. Poll the pod until the SSH port is forwarded.
//!   5. rsync the project workdir onto the pod.
//!   6. SSH in and run setup + run blocks from the manifest, streaming
//!      stdout/stderr back to the local terminal.
//!   7. On clean exit, optionally rsync the configured `outputs` paths
//!      back, then terminate the pod.
//!   8. On Ctrl-C, attempt termination and exit with a non-zero code.
//!
//! The Rust side handles the API + orchestration. Transport (ssh,
//! rsync) shells out to the host tools that every dev machine already
//! has. The typed API client lives in the sibling `runpod` crate
//! (see ../api/).

mod cli;
mod config;
mod manifest;
mod pod;
mod ssh;
mod state;
mod template;
mod train;
mod ui_text;
mod volume;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    // Load project-local .env if present. We deliberately swallow the
    // "not found" case — many invocations won't have a .env in cwd
    // (e.g. running on the pod itself). Other errors surface as
    // warnings so a malformed .env doesn't hide misconfiguration.
    match dotenvy::dotenv() {
        Ok(path) => eprintln!("[runpod-cli] loaded env from {}", path.display()),
        Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("[runpod-cli] .env load failed: {e}"),
    }

    let args = cli::Cli::parse();
    cli::run(args).await
}
