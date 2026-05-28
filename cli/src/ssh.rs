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
        // Keepalive hardening. RunPod's connection proxy resets
        // idle-looking long-lived sessions; these reduce (but don't
        // eliminate) mid-run drops. The real fix for drops is
        // detached execution (`run_remote_script_detached`), which
        // decouples the training lifetime from this SSH session.
        "-o".into(), "ServerAliveInterval=15".into(),
        "-o".into(), "ServerAliveCountMax=8".into(),
        "-o".into(), "TCPKeepAlive=yes".into(),
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

/// Launch `script` **detached** on the pod via `nohup`, so it
/// survives the SSH session that started it. Returns as soon as
/// the process is backgrounded; the caller polls for completion
/// with [`poll_remote_until_done`].
///
/// Pod-side layout (all under `/root/workdir/`):
///   * `.agora_run.sh`   — the script body (envs + manifest `run`)
///   * `.agora_run.log`  — combined stdout/stderr
///   * `.agora_run.pid`  — backgrounded shell's PID
///   * `.agora_run.exit` — written with the exit code on completion
///
/// The `.exit` sentinel is the completion signal; its contents are
/// the script's exit status. This is the fix for the SSH-drop
/// failure mode: a dropped connection mid-training no longer kills
/// the run (nohup detaches it from the session's controlling
/// terminal) and no longer loses progress (the run keeps
/// checkpointing; the next poll reconnects and pulls).
pub fn run_remote_script_detached(
    endpoint: &SshEndpoint,
    name: &str,
    script: &str,
    envs: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    if script.trim().is_empty() {
        return Ok(());
    }
    eprintln!("[runpod-cli] launching `{name}` detached on pod");
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(endpoint));
    cmd.arg("bash -se");
    cmd.stdin(std::process::Stdio::piped());
    let mut child = cmd.spawn().context("spawning ssh (detached launch)")?;
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().context("ssh stdin")?;
        // Build the run-script file on the pod, then nohup it.
        stdin.write_all(b"set -euo pipefail\ncd /root/workdir\n")?;
        // Reset any stale sentinels from a prior attempt.
        stdin.write_all(
            b"rm -f .agora_run.exit .agora_run.pid .agora_run.log\n",
        )?;
        // Heredoc the script body into .agora_run.sh. Use a quoted
        // delimiter so the body is written verbatim (no expansion
        // at write time — the script runs later under its own bash).
        stdin.write_all(b"cat > .agora_run.sh <<'AGORA_RUN_BODY_EOF'\n")?;
        stdin.write_all(b"set -euo pipefail\ncd /root/workdir\n")?;
        for (k, v) in envs {
            let escaped = v.replace('\'', "'\\''");
            writeln!(stdin, "export {k}='{escaped}'")?;
        }
        stdin.write_all(script.as_bytes())?;
        stdin.write_all(b"\nAGORA_RUN_BODY_EOF\n")?;
        // Launch detached: run the body, capture its exit code into
        // the sentinel. `setsid` fully divorces it from any
        // controlling terminal so an SSH hang-up can't signal it.
        stdin.write_all(
            b"setsid bash -c 'bash .agora_run.sh > .agora_run.log 2>&1; \
              echo $? > .agora_run.exit' < /dev/null > /dev/null 2>&1 &\n",
        )?;
        stdin.write_all(b"echo $! > .agora_run.pid\n")?;
        stdin.write_all(b"echo \"[pod] detached run launched, pid=$(cat .agora_run.pid)\"\n")?;
    }
    let status = child.wait().context("waiting on ssh (detached launch)")?;
    if !status.success() {
        bail!("detached launch of `{name}` failed with status {status}");
    }
    Ok(())
}

/// Poll the pod until the detached run writes its `.agora_run.exit`
/// sentinel, reconnecting on each tick so an individual SSH drop is
/// harmless. Streams new log lines between polls. Returns the
/// script's exit code.
pub fn poll_remote_until_done(
    endpoint: &SshEndpoint,
    poll_secs: u64,
) -> Result<i32> {
    use std::io::Write;
    let mut last_log_lines: u64 = 0;
    loop {
        // One poll = one fresh SSH connection. A transient failure
        // here is non-fatal: we just retry on the next tick.
        let probe = run_capture(
            endpoint,
            // Emit: exit-sentinel (or empty), total log line count,
            // and any log lines past what we've already shown.
            &format!(
                "cd /root/workdir 2>/dev/null || exit 0; \
                 EXIT=$(cat .agora_run.exit 2>/dev/null || echo ''); \
                 LINES=$(wc -l < .agora_run.log 2>/dev/null || echo 0); \
                 echo \"AGORA_EXIT=${{EXIT}}\"; \
                 echo \"AGORA_LINES=${{LINES}}\"; \
                 echo 'AGORA_TAIL<<<'; \
                 tail -n +{} .agora_run.log 2>/dev/null | tail -n 40; \
                 echo '>>>'",
                last_log_lines + 1,
            ),
        );
        match probe {
            Ok(out) => {
                let mut exit_val: Option<i32> = None;
                let mut in_tail = false;
                for line in out.lines() {
                    if let Some(rest) = line.strip_prefix("AGORA_EXIT=") {
                        let rest = rest.trim();
                        if !rest.is_empty() {
                            exit_val = rest.parse::<i32>().ok().or(Some(0));
                        }
                    } else if let Some(rest) = line.strip_prefix("AGORA_LINES=") {
                        if let Ok(n) = rest.trim().parse::<u64>() {
                            last_log_lines = n;
                        }
                    } else if line == "AGORA_TAIL<<<" {
                        in_tail = true;
                    } else if line == ">>>" {
                        in_tail = false;
                    } else if in_tail {
                        // Stream pod log to our stderr.
                        let _ = writeln!(std::io::stderr(), "{line}");
                    }
                }
                if let Some(code) = exit_val {
                    eprintln!("[runpod-cli] detached run finished, exit={code}");
                    return Ok(code);
                }
            }
            Err(e) => {
                eprintln!("[runpod-cli] poll tick failed ({e}); retrying in {poll_secs}s");
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(poll_secs));
    }
}

/// Run a command on the pod, capturing stdout as a String. Used by
/// the poll loop.
fn run_capture(endpoint: &SshEndpoint, cmd_line: &str) -> Result<String> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(endpoint));
    cmd.arg(cmd_line);
    let out = cmd.output().context("spawning ssh (capture)")?;
    if !out.status.success() {
        bail!("remote capture exited with status {}", out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
