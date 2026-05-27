//! Loader for the API key. Mirrors the CLI's config loader but skips
//! the SSH-key bits (the TUI never shells out to ssh).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
struct ConfigFile {
    api_key: Option<String>,
}

pub struct Config {
    pub api_key: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_file_path();
        let from_file: ConfigFile = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            toml_str_parse(&text)?
        } else {
            ConfigFile::default()
        };
        let api_key = std::env::var("RUNPOD_API_KEY")
            .ok()
            .or(from_file.api_key)
            .context("RUNPOD_API_KEY not set in env, .env, or config file")?;
        Ok(Self { api_key })
    }
}

fn config_file_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("", "", "runpod-cli") {
        dirs.config_dir().join("config.toml")
    } else {
        PathBuf::from(".runpod-cli.toml")
    }
}

fn toml_str_parse(text: &str) -> Result<ConfigFile> {
    // Single-purpose; pulling in the workspace `toml` crate just for
    // this would be overkill, but cli's config already depends on it
    // so it's "free" here.
    Ok(toml_basic(text))
}

/// Minimal TOML key-extractor — looks for `api_key = "..."` only.
/// Avoids the toml crate in the TUI dep set since this loader stays
/// intentionally tiny.
fn toml_basic(text: &str) -> ConfigFile {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("api_key") {
            let rest = rest.trim_start_matches(|c: char| c == '=' || c.is_whitespace());
            let v = rest.trim_matches('"');
            return ConfigFile { api_key: Some(v.to_string()) };
        }
    }
    ConfigFile::default()
}
