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
use serde::{Deserialize, Deserializer};
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
    /// Inputs to stage onto the mounted network volume via the S3 API
    /// *before* deploy — the fast data path for bulk data (datasets,
    /// model weights) that bypasses the SSH workdir rsync. Requires
    /// `resources.network_volume_id`. The pod mounts the volume (default
    /// `/workspace`), so a staged key `k` is readable at `/workspace/k`.
    #[serde(default)]
    pub stage: Vec<StageEntry>,

    pub resources: Resources,
    #[serde(default)]
    pub envs: BTreeMap<String, String>,
    /// Names of env vars to copy from the local environment into the
    /// pod's env at deploy time. Use for secrets that don't belong in
    /// the manifest (WANDB_API_KEY, HF_TOKEN, …).
    #[serde(default)]
    pub forward_envs: Vec<String>,
    /// Run the `run` script detached (`nohup`/`setsid`) on the pod
    /// and poll for completion, rather than streaming it over a
    /// single long-lived SSH session. Strongly recommended for any
    /// run longer than a few minutes: a dropped SSH connection no
    /// longer kills the run or loses progress. Defaults to false
    /// for backward compatibility; long jobs (training) set true.
    #[serde(default)]
    pub detached: bool,
    /// Poll interval (seconds) for the detached run's completion
    /// check. Ignored unless `detached`. Default 30.
    #[serde(default = "default_poll_secs")]
    pub poll_secs: u64,

    pub setup: String,
    pub run: String,
}

/// One input to stage onto a network volume before deploy.
#[derive(Debug, Clone, Deserialize)]
pub struct StageEntry {
    /// Local path (file or directory) to upload.
    pub local: String,
    /// Key prefix on the volume (e.g. `datasets/parser`).
    #[serde(default)]
    pub key: String,
}

fn default_poll_secs() -> u64 { 30 }

#[derive(Debug, Clone, Deserialize)]
pub struct Resources {
    /// GPU type(s) to provision. Accepts either a single string
    /// (`gpu_type = "NVIDIA A40"`) or an ordered fallback list
    /// (`gpu_type = ["NVIDIA L40S", "NVIDIA A40"]`); `train` tries each
    /// in turn, advancing past capacity ("no instances available")
    /// errors. Always normalized to a `Vec` here.
    #[serde(deserialize_with = "string_or_vec")]
    pub gpu_type: Vec<String>,
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
    /// Deploy from a registered RunPod template id (image / ports / disk
    /// / start command come from the template; `image` is then ignored).
    /// The GPU type is still chosen per-deploy, so the fallback applies.
    #[serde(default)]
    pub template_id: Option<String>,
}

/// Accept a single string or a sequence of strings for `gpu_type`,
/// normalizing both to `Vec<String>` (backward compatible with the
/// historical single-string manifests).
fn string_or_vec<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        One(String),
        Many(Vec<String>),
    }
    Ok(match StringOrVec::deserialize(d)? {
        StringOrVec::One(s) => vec![s],
        StringOrVec::Many(v) => v,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(resources_gpu: &str) -> Manifest {
        let text = format!(
            "name = \"x\"\nsetup = \"\"\nrun = \"\"\n[resources]\n{resources_gpu}\nimage = \"img\"\n"
        );
        toml::from_str(&text).expect("manifest should parse")
    }

    #[test]
    fn gpu_type_accepts_single_string() {
        let m = parse("gpu_type = \"NVIDIA A40\"");
        assert_eq!(m.resources.gpu_type, vec!["NVIDIA A40".to_string()]);
    }

    #[test]
    fn gpu_type_accepts_fallback_list() {
        let m = parse("gpu_type = [\"NVIDIA L40S\", \"NVIDIA A40\"]");
        assert_eq!(
            m.resources.gpu_type,
            vec!["NVIDIA L40S".to_string(), "NVIDIA A40".to_string()]
        );
    }
}
