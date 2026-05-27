//! Manifest file format. Mirrors the SkyPilot-style YAML in spirit but
//! is parsed from TOML so we don't need a YAML crate.
//!
//! Example runpod.toml:
//!
//!     name = "prime-transformer"
//!     workdir = "."
//!     outputs = ["runs"]
//!
//!     [resources]
//!     gpu_type = "NVIDIA A100 80GB PCIe"
//!     container_disk_gb = 50
//!     volume_gb = 200
//!     image = "runpod/pytorch:2.4.0-py3.11-cuda12.4.1-devel-ubuntu22.04"
//!     cloud_type = "COMMUNITY"  # or "SECURE"
//!
//!     [envs]
//!     WANDB_PROJECT = "prime-transformer"
//!     TASK_CONFIG = "configs/next_prime_medium.yaml"
//!
//!     setup = """
//!     curl -fsSL -o /usr/local/bin/bazel ...
//!     ...
//!     """
//!
//!     run = """
//!     bazel run //prime_transformer:train -- ...
//!     """

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub name: String,
    #[serde(default = "default_workdir")]
    pub workdir: PathBuf,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub skip_patterns: Vec<String>,

    pub resources: Resources,
    #[serde(default)]
    pub envs: BTreeMap<String, String>,
    /// Names of env vars to copy from the local environment into the
    /// pod's env at deploy time. Use for secrets that don't belong in
    /// the manifest (WANDB_API_KEY, HF_TOKEN, …).
    #[serde(default)]
    pub forward_envs: Vec<String>,

    pub setup: String,
    pub run: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resources {
    pub gpu_type: String,
    #[serde(default = "default_container_disk")]
    pub container_disk_gb: u32,
    #[serde(default = "default_volume")]
    pub volume_gb: u32,
    pub image: String,
    #[serde(default = "default_cloud_type")]
    pub cloud_type: String,
    #[serde(default = "default_wait_timeout")]
    pub wait_timeout_secs: u64,
    /// Existing network volume id to mount on the pod (persists across
    /// pod restarts). Pod must be in the volume's data center.
    #[serde(default)]
    pub network_volume_id: Option<String>,
    /// Single data center id (e.g. "US-IL-1") to constrain placement
    /// to. Required when network_volume_id is set; the volume can only
    /// be mounted by pods in its own DC.
    #[serde(default)]
    pub data_center_id: Option<String>,
}

fn default_workdir() -> PathBuf { PathBuf::from(".") }
fn default_container_disk() -> u32 { 50 }
fn default_volume() -> u32 { 200 }
fn default_cloud_type() -> String { "COMMUNITY".into() }
fn default_wait_timeout() -> u64 { 600 }

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let m: Manifest = toml::from_str(&text).context("parsing manifest")?;
        Ok(m)
    }
}
