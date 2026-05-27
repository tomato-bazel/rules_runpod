//! Wrapper around the progenitor-generated RunPod REST client.
//!
//! The generated `runpod_api::Client` exposes one method per OpenAPI
//! operation with strongly-typed inputs / outputs. We thin-wrap it
//! here for two reasons:
//!
//! 1. **Auth.** Bearer-token authentication is added once via a
//!    pre-baked `reqwest::Client` so call sites don't have to set the
//!    header on every request.
//! 2. **Surface.** Most of the generated 30-plus operations aren't
//!    needed by runpod-cli; the methods on this wrapper are the
//!    project-specific verbs (deploy / get / wait / terminate)
//!    consumed by `cli.rs`.
//!
//! Errors from the generated client are flattened into `anyhow::Error`
//! with status-code context so failures surface as a single message in
//! the CLI rather than the progenitor `Error<()>` enum.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use runpod_api::types::{
    Pod as ApiPod, PodCreateInput, PodCreateInputCloudType, PodCreateInputName,
    PodDesiredStatus,
};

// Re-export the generated typed surface so downstream code can write
// `use runpod::types::Pod` and `use runpod::Error` without depending
// on the rules_openapi target name.
pub use runpod_api::Error;
pub use runpod_api::ResponseValue;
pub use runpod_api::types;

const BASE_URL: &str = "https://rest.runpod.io/v1";

pub struct Client {
    inner: runpod_api::Client,
}

impl Client {
    pub fn new(api_key: String) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        let mut auth = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))
            .context("constructing Authorization header")?;
        auth.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(60))
            .build()
            .context("building reqwest client")?;
        let inner = runpod_api::Client::new_with_client(BASE_URL, http);
        Ok(Self { inner })
    }

    /// Escape hatch for callers that want to use a generated method we
    /// haven't wrapped — e.g. `client.inner().list_endpoints(...)`.
    pub fn inner(&self) -> &runpod_api::Client {
        &self.inner
    }

    // ---------------------------------------------------------------------
    // Pods
    // ---------------------------------------------------------------------

    pub async fn list_pods(&self) -> Result<Vec<ApiPod>> {
        let resp = self
            .inner
            .list_pods(None, None, None, None, None, None, None, None, None,
                       None, None, None, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("list_pods: {e}"))?;
        Ok(resp.into_inner().0)
    }

    /// Full generated `Pod` (~30 fields). For projects that only need
    /// id/status/IP/port-mapping, use `get_pod` which returns the
    /// project-shaped wrapper.
    pub async fn get_pod_full(&self, pod_id: &str) -> Result<ApiPod> {
        let resp = self
            .inner
            .get_pod(pod_id, None, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("get_pod {pod_id}: {e}"))?;
        Ok(resp.into_inner())
    }

    pub async fn start_pod(&self, pod_id: &str) -> Result<()> {
        self.inner
            .start_pod(pod_id)
            .await
            .map_err(|e| anyhow::anyhow!("start_pod {pod_id}: {e}"))?;
        Ok(())
    }

    pub async fn stop_pod(&self, pod_id: &str) -> Result<()> {
        self.inner
            .stop_pod(pod_id)
            .await
            .map_err(|e| anyhow::anyhow!("stop_pod {pod_id}: {e}"))?;
        Ok(())
    }

    pub async fn restart_pod(&self, pod_id: &str) -> Result<()> {
        self.inner
            .restart_pod(pod_id)
            .await
            .map_err(|e| anyhow::anyhow!("restart_pod {pod_id}: {e}"))?;
        Ok(())
    }

    pub async fn reset_pod(&self, pod_id: &str) -> Result<()> {
        self.inner
            .reset_pod(pod_id)
            .await
            .map_err(|e| anyhow::anyhow!("reset_pod {pod_id}: {e}"))?;
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Templates
    // ---------------------------------------------------------------------

    pub async fn list_templates(&self) -> Result<Vec<types::Template>> {
        let resp = self
            .inner
            .list_templates(None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("list_templates: {e}"))?;
        Ok(resp.into_inner().0)
    }

    pub async fn get_template(&self, id: &str) -> Result<types::Template> {
        let resp = self
            .inner
            .get_template(id, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("get_template {id}: {e}"))?;
        Ok(resp.into_inner())
    }

    pub async fn delete_template(&self, id: &str) -> Result<()> {
        self.inner
            .delete_template(id)
            .await
            .map_err(|e| anyhow::anyhow!("delete_template {id}: {e}"))?;
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Network volumes
    // ---------------------------------------------------------------------

    pub async fn list_network_volumes(&self) -> Result<Vec<types::NetworkVolumesItem>> {
        let resp = self
            .inner
            .list_network_volumes()
            .await
            .map_err(|e| anyhow::anyhow!("list_network_volumes: {e}"))?;
        Ok(resp.into_inner().0)
    }

    pub async fn get_network_volume(&self, id: &str) -> Result<types::NetworkVolume> {
        let resp = self
            .inner
            .get_network_volume(id)
            .await
            .map_err(|e| anyhow::anyhow!("get_network_volume {id}: {e}"))?;
        Ok(resp.into_inner())
    }

    pub async fn delete_network_volume(&self, id: &str) -> Result<()> {
        self.inner
            .delete_network_volume(id)
            .await
            .map_err(|e| anyhow::anyhow!("delete_network_volume {id}: {e}"))?;
        Ok(())
    }

    pub async fn create_network_volume(
        &self,
        body: &types::NetworkVolumeCreateInput,
    ) -> Result<types::NetworkVolume> {
        let resp = self
            .inner
            .create_network_volume(body)
            .await
            .map_err(|e| anyhow::anyhow!("create_network_volume: {e}"))?;
        Ok(resp.into_inner())
    }

    pub async fn create_template(
        &self,
        body: &types::TemplateCreateInput,
    ) -> Result<types::Template> {
        let resp = self
            .inner
            .create_template(body)
            .await
            .map_err(|e| anyhow::anyhow!("create_template: {e}"))?;
        Ok(resp.into_inner())
    }

    // ---------------------------------------------------------------------
    // Project-shaped pod ops (used by the train flow)
    // ---------------------------------------------------------------------

    /// Extract a useful error message from a progenitor `Error<E>`.
    /// `UnexpectedResponse` carries the raw reqwest::Response — its
    /// body is what we usually need to debug a 4xx/5xx from upstream.
    async fn handle_progenitor_err<E>(
        op: &str,
        e: runpod_api::Error<E>,
    ) -> anyhow::Error
    where
        E: std::fmt::Debug,
    {
        match e {
            runpod_api::Error::UnexpectedResponse(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                anyhow::anyhow!("{op}: HTTP {status}: {body}")
            }
            other => anyhow::anyhow!("{op}: {other}"),
        }
    }

    pub async fn deploy_on_demand(&self, req: &DeployRequest) -> Result<Pod> {
        let mut env: HashMap<String, String> = req.env_as_hashmap();
        env.insert("PUBLIC_KEY".into(), req.public_key.clone());

        let cloud_type = match req.cloud_type.as_str() {
            "SECURE" => PodCreateInputCloudType::Secure,
            // Anything else (including "COMMUNITY", the default) falls
            // through to Community Cloud.
            _ => PodCreateInputCloudType::Community,
        };

        let name = PodCreateInputName::try_from(req.name.as_str())
            .map_err(|e| anyhow::anyhow!("invalid pod name {:?}: {e}", req.name))?;

        let data_center_ids: Vec<runpod_api::types::PodCreateInputDataCenterIdsItem> =
            if let Some(dc) = req.data_center_id.as_deref() {
                vec![dc
                    .parse()
                    .map_err(|e| anyhow::anyhow!("invalid data_center_id {dc:?}: {e}"))?]
            } else {
                Vec::new()
            };
        let mut body = PodCreateInput {
            name,
            image_name: Some(req.image.clone()),
            container_disk_in_gb: Some(req.container_disk_gb as i64),
            volume_in_gb: Some(req.volume_gb as i64),
            cloud_type,
            env,
            ports: req.ports.split(',').map(|s| s.trim().to_string()).collect(),
            gpu_type_ids: vec![req
                .gpu_type
                .as_str()
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid gpu_type {:?}: {e}", req.gpu_type))?],
            gpu_count: std::num::NonZeroU64::new(req.gpu_count as u64)
                .context("gpu_count must be > 0")?,
            network_volume_id: req.network_volume_id.clone(),
            ..Default::default()
        };
        if !data_center_ids.is_empty() {
            body.data_center_ids = data_center_ids;
        }

        let resp = match self.inner.create_pod(&body).await {
            Ok(r) => r,
            Err(e) => return Err(Self::handle_progenitor_err("create_pod", e).await),
        };
        Ok(Pod::from_api(resp.into_inner()))
    }

    pub async fn get_pod(&self, pod_id: &str) -> Result<Pod> {
        let resp = self
            .inner
            .get_pod(pod_id, None, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("get_pod {pod_id}: {e}"))?;
        Ok(Pod::from_api(resp.into_inner()))
    }

    pub async fn terminate_pod(&self, pod_id: &str) -> Result<()> {
        self.inner
            .delete_pod(pod_id)
            .await
            .map_err(|e| anyhow::anyhow!("delete_pod {pod_id}: {e}"))?;
        Ok(())
    }

    /// Poll the pod until SSH is reachable. Returns the running Pod.
    pub async fn wait_for_ssh(&self, pod_id: &str, timeout_secs: u64) -> Result<Pod> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let mut last_status = String::new();
        loop {
            let pod = self.get_pod(pod_id).await?;
            if pod.ssh_endpoint().is_some()
                && matches!(pod.desired_status, Some(PodDesiredStatus::Running))
            {
                return Ok(pod);
            }
            let status = pod
                .desired_status
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "UNKNOWN".into());
            if status != last_status {
                eprintln!("[runpod-cli] pod status: {status}");
                last_status = status.clone();
            }
            if start.elapsed() > timeout {
                bail!(
                    "timed out after {timeout_secs}s waiting for ssh on pod {pod_id} \
                     (status: {status})"
                );
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

/// Project-facing pod model. The generated `runpod_api::types::Pod` has
/// ~30 optional fields; we project the handful actually used by the
/// CLI. Keep this small — anything not on this struct goes through
/// the wrapper Client's method surface.
#[derive(Debug, Clone)]
pub struct Pod {
    pub id: String,
    pub desired_status: Option<PodDesiredStatus>,
    /// Empty string until the pod has been assigned a public IP.
    /// Stored as the raw String we received because RunPod uses ""
    /// as a "not yet" sentinel.
    pub public_ip: Option<String>,
    /// `{ "22": 12345, "8080": 23456 }` — internal port to public port.
    pub port_mappings: HashMap<String, i64>,
}

impl Pod {
    fn from_api(p: ApiPod) -> Self {
        Self {
            id: p.id.unwrap_or_default(),
            desired_status: p.desired_status,
            // Treat empty string as "no IP yet" so the rest of the
            // codebase can `.is_some()` without checking emptiness.
            public_ip: p.public_ip.filter(|s| !s.is_empty()),
            port_mappings: p.port_mappings.unwrap_or_default(),
        }
    }

    /// `Some` once the pod has a public IP and port 22 has been mapped.
    pub fn ssh_endpoint(&self) -> Option<SshEndpoint> {
        let ip = self.public_ip.as_deref()?;
        let port = *self.port_mappings.get("22")?;
        if port <= 0 || port > u16::MAX as i64 {
            return None;
        }
        Some(SshEndpoint {
            host: ip.to_string(),
            port: port as u32,
            user: "root".into(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SshEndpoint {
    pub host: String,
    pub port: u32,
    pub user: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeployRequest {
    pub gpu_type: String,
    pub gpu_count: u32,
    pub container_disk_gb: u32,
    pub volume_gb: u32,
    pub image: String,
    /// Comma-separated `<port>/<proto>` list, e.g. `"22/tcp,8080/http"`.
    pub ports: String,
    pub name: String,
    pub public_key: String,
    pub cloud_type: String,
    pub env: std::collections::BTreeMap<String, String>,
    /// Optional existing network volume to mount. Pod must be in the
    /// volume's data center.
    pub network_volume_id: Option<String>,
    /// Constrain placement to a single data center, e.g. "US-IL-1".
    pub data_center_id: Option<String>,
}

impl DeployRequest {
    /// Convenience for cli.rs — env starts as BTreeMap for stable
    /// serialization but the generated client wants a HashMap.
    pub fn env_as_hashmap(&self) -> HashMap<String, String> {
        self.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}
