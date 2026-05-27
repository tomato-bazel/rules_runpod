//! SSH + rsync transport. Shells out to the host's `ssh` and `rsync`
//! binaries rather than reimplementing them; every dev box has them
//! and they handle host-key prompts, agent forwarding, and concurrency
//! semantics that an in-process implementation would have to replicate.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

use runpod::SshEndpoint;

/// Read `<key>.pub` next to the configured private key. If neither
/// exists, generate a fresh ed25519 keypair (so first-time users
/// aren't blocked).
pub fn read_or_generate_pubkey(key_path: &Path) -> Result<String> {
    let pub_path = pub_key_path(key_path);
    if !pub_path.exists() {
        if !key_path.exists() {
            eprintln!("[runpod-cli] no ssh key at {} — generating ed25519", key_path.display());
            run_local(&["ssh-keygen", "-t", "ed25519", "-N", "", "-f",
                        key_path.to_str().context("ssh key path is not utf-8")?])?;
        } else {
            bail!("private key {} has no matching .pub", key_path.display());
        }
    }
    let text = std::fs::read_to_string(&pub_path)
        .with_context(|| format!("reading {}", pub_path.display()))?;
    Ok(text.trim().to_string())
}

fn pub_key_path(key_path: &Path) -> PathBuf {
    let mut s = key_path.as_os_str().to_owned();
    s.push(".pub");
    PathBuf::from(s)
}

/// Common ssh flags. Disable host key checking the first time because
/// RunPod pods are ephemeral and don't have stable host keys; that
/// trade-off is the price of automation here.
fn ssh_args(endpoint: &SshEndpoint) -> Vec<String> {
    vec![
        "-p".to_string(), endpoint.port.to_string(),
        "-o".into(), "StrictHostKeyChecking=no".into(),
        "-o".into(), "UserKnownHostsFile=/dev/null".into(),
        "-o".into(), "LogLevel=ERROR".into(),
        "-o".into(), "ServerAliveInterval=30".into(),
        format!("{}@{}", endpoint.user, endpoint.host),
    ]
}

pub fn rsync_up(endpoint: &SshEndpoint, local: &Path, remote: &str,
                skip: &[String]) -> Result<()> {
    ensure_remote_rsync(endpoint)?;
    eprintln!("[runpod-cli] rsync {} -> {remote}", local.display());
    // -az = archive + compress. macOS ships an old BSD rsync without
    // --info=progress2, so we stick to plain --progress for portability.
    let mut args: Vec<String> = vec![
        "-az".into(), "--progress".into(), "--delete-after".into(),
        "-e".into(),
        format!("ssh -p {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR",
                endpoint.port),
    ];
    for pat in default_skips().iter().chain(skip.iter()) {
        args.push("--exclude".into());
        args.push(pat.clone());
    }
    args.push(format!("{}/", local.display()));
    args.push(format!("{}@{}:{}", endpoint.user, endpoint.host, remote));
    run_local_owned("rsync", &args)
}

pub fn rsync_down(endpoint: &SshEndpoint, remote: &str, local: &Path) -> Result<()> {
    ensure_remote_rsync(endpoint)?;
    eprintln!("[runpod-cli] rsync {remote} -> {}", local.display());
    let args = vec![
        "-az".to_string(), "--progress".into(),
        "-e".into(),
        format!("ssh -p {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR",
                endpoint.port),
        format!("{}@{}:{}", endpoint.user, endpoint.host, remote),
        local.display().to_string(),
    ];
    run_local_owned("rsync", &args)
}

pub fn run_remote_script(endpoint: &SshEndpoint, name: &str, script: &str) -> Result<()> {
    run_remote_script_env(endpoint, name, script, &std::collections::BTreeMap::new())
}

/// Run a remote bash script over ssh. `envs` are exported in the
/// shell preamble so the script can refer to them with `$NAME`.
/// RunPod's pod-creation env doesn't propagate into non-interactive
/// ssh sessions reliably, so we re-export here.
pub fn run_remote_script_env(
    endpoint: &SshEndpoint,
    name: &str,
    script: &str,
    envs: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    if script.trim().is_empty() {
        return Ok(());
    }
    eprintln!("[runpod-cli] running `{name}` on pod");
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(endpoint));
    cmd.arg("bash -se");        // -s: read from stdin; -e: fail-fast
    cmd.stdin(std::process::Stdio::piped());
    let mut child = cmd.spawn().context("spawning ssh")?;
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().context("ssh stdin")?;
        stdin.write_all(b"set -euo pipefail\ncd /root/workdir\n")?;
        for (k, v) in envs {
            // Single-quote the value, escaping any embedded single quotes.
            let escaped = v.replace('\'', "'\\''");
            writeln!(stdin, "export {k}='{escaped}'")?;
        }
        stdin.write_all(script.as_bytes())?;
    }
    let status = child.wait().context("waiting on ssh")?;
    if !status.success() {
        bail!("remote `{name}` exited with status {status}");
    }
    Ok(())
}

pub fn exec_remote(endpoint: &SshEndpoint, cmd_line: &str) -> Result<()> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(endpoint));
    cmd.arg(format!("cd /root/workdir && {cmd_line}"));
    let status = cmd.status().context("spawning ssh")?;
    if !status.success() {
        bail!("remote command exited with status {status}");
    }
    Ok(())
}

pub fn interactive_ssh(endpoint: &SshEndpoint) -> Result<()> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(endpoint));
    cmd.arg("cd /root/workdir 2>/dev/null; exec bash -l");
    let status = cmd.status().context("spawning ssh")?;
    std::process::exit(status.code().unwrap_or(1));
}

/// RunPod's pytorch image doesn't ship rsync. ssh in and install it
/// on first call; subsequent calls are no-ops because the container
/// disk persists across our rsync invocations.
fn ensure_remote_rsync(endpoint: &SshEndpoint) -> Result<()> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(endpoint));
    cmd.arg(
        "command -v rsync >/dev/null 2>&1 || (apt-get update -qq && apt-get install -yqq rsync)",
    );
    let status = cmd.status().context("spawning ssh to install rsync")?;
    if !status.success() {
        bail!("failed to ensure rsync on remote (status {})", status);
    }
    Ok(())
}

fn run_local(args: &[&str]) -> Result<()> {
    let (prog, rest) = args.split_first().context("empty command")?;
    let status = Command::new(prog).args(rest).status()
        .with_context(|| format!("spawning {prog}"))?;
    if !status.success() {
        bail!("{prog} exited with {status}");
    }
    Ok(())
}

fn run_local_owned(prog: &str, args: &[String]) -> Result<()> {
    let status = Command::new(prog).args(args).status()
        .with_context(|| format!("spawning {prog}"))?;
    if !status.success() {
        bail!("{prog} exited with {status}");
    }
    Ok(())
}

/// Patterns rsync should always skip — equivalent in spirit to a
/// .gitignore but trimmed to the entries that would either fail to
/// transfer (bazel symlinks pointing outside the workspace) or pay
/// no benefit (the local pip cache, run dirs).
fn default_skips() -> Vec<String> {
    vec![
        "bazel-*".into(),
        ".git/".into(),
        "__pycache__/".into(),
        "*.pyc".into(),
        ".venv/".into(),
        ".uv-cache/".into(),
        "data/shards/".into(),
        "runs/".into(),
        "wandb/".into(),
        "*.pt".into(),
        "*.ckpt".into(),
        ".DS_Store".into(),
        ".vscode/".into(),
        ".idea/".into(),
        ".runpod/".into(),
    ]
}
