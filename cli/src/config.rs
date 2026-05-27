//! User-level config: API key + ssh key path. Read once from
//! ~/.config/runpod-cli/config.toml or from RUNPOD_API_KEY env var.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigFile {
    pub api_key: Option<String>,
    pub ssh_key_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    // Lazily resolved by `ssh_key_path()` — only `train` actually needs
    // an SSH key, and we don't want `status` / `down` to fail just
    // because the user hasn't generated one yet.
    file_ssh_key_path: Option<PathBuf>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let file = config_file_path();
        let file_cfg: ConfigFile = if file.exists() {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("reading {}", file.display()))?;
            toml::from_str(&text).context("parsing config.toml")?
        } else {
            ConfigFile { api_key: None, ssh_key_path: None }
        };
        let api_key = std::env::var("RUNPOD_API_KEY")
            .ok()
            .or(file_cfg.api_key)
            .context(
                "RUNPOD_API_KEY not set in env, .env, or ~/.config/runpod-cli/config.toml",
            )?;
        Ok(Self { api_key, file_ssh_key_path: file_cfg.ssh_key_path })
    }

    /// Resolve the SSH key path. Returns the configured value, then
    /// falls back to `~/.ssh/id_ed25519` / `~/.ssh/id_rsa`. If none
    /// exist, returns the ed25519 path as a hint where `train` will
    /// generate one.
    pub fn ssh_key_path(&self) -> Result<PathBuf> {
        if let Some(p) = &self.file_ssh_key_path {
            return Ok(p.clone());
        }
        if let Some(p) = default_ssh_key() {
            return Ok(p);
        }
        // Synthesize the default; train will generate if missing.
        let home = directories::UserDirs::new()
            .context("cannot resolve home directory")?
            .home_dir()
            .to_path_buf();
        Ok(home.join(".ssh").join("id_ed25519"))
    }
}

fn config_file_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("", "", "runpod-cli") {
        dirs.config_dir().join("config.toml")
    } else {
        PathBuf::from(".runpod-cli.toml")
    }
}

fn default_ssh_key() -> Option<PathBuf> {
    let home = directories::UserDirs::new()?.home_dir().to_path_buf();
    for candidate in ["id_ed25519", "id_rsa"] {
        let p = home.join(".ssh").join(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}
