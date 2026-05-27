//! Persistent state about the currently-active pod, kept in
//! `.runpod/state.toml` so `runpod-cli down` and friends can find it
//! across invocations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub pod_id: Option<String>,
}

impl State {
    pub fn load_or_default() -> Result<Self> {
        let p = state_path();
        if !p.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        Ok(toml::from_str(&text).context("parsing state.toml")?)
    }

    pub fn save(&self) -> Result<()> {
        let p = state_path();
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).context("creating .runpod/")?;
        }
        let text = toml::to_string_pretty(self).context("serializing state")?;
        std::fs::write(&p, text).with_context(|| format!("writing {}", p.display()))?;
        Ok(())
    }

    pub fn clear() -> Result<()> {
        let p = state_path();
        if p.exists() {
            std::fs::remove_file(&p).with_context(|| format!("removing {}", p.display()))?;
        }
        Ok(())
    }
}

fn state_path() -> PathBuf {
    PathBuf::from(".runpod").join("state.toml")
}
