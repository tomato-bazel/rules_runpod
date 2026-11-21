"""rules_runpod private rule implementations.

The macros in //runpod:defs.bzl are thin wrappers around these
rules. Public surface is the macros; rule shapes can evolve.

The shape mirrors `rules_lora`: build-time emits a typed JSON
spec; execute-time is the `runpod-cli` consuming the spec.
"""

load(
    ":providers.bzl",
    "RunpodJobInfo",
    "RunpodManifestInfo",
    "RunpodNetworkVolumeInfo",
    "RunpodPodInfo",
    "RunpodServerlessEndpointInfo",
    "RunpodTemplateInfo",
)

# ============================================================
# runpod_manifest — validate + sha-pin a TOML manifest.
# ============================================================
def _runpod_manifest_impl(ctx):
    # v0.0.1: pass-through validation (the CLI's manifest parser
    # rejects malformed TOML at run time). v0.0.2 promotes the
    # parse to a build-time check via a tiny rust_binary that
    # verifies the manifest against the typed `Manifest` struct
    # in `//cli/src/manifest.rs`.
    return [
        DefaultInfo(files = depset([ctx.file.src])),
        RunpodManifestInfo(
            toml = ctx.file.src,
            sha = "",  # filled at v0.0.2 by the validator
            workdir = ctx.attr.workdir,
            outputs = ctx.attr.outputs,
        ),
    ]

runpod_manifest = rule(
    implementation = _runpod_manifest_impl,
    attrs = {
        "src": attr.label(
            mandatory = True,
            allow_single_file = [".toml"],
            doc = "Path to the manifest TOML.",
        ),
        "workdir": attr.string(
            default = ".",
            doc = "Workspace-relative path to rsync to the pod.",
        ),
        "outputs": attr.string_list(
            doc = "Paths under the pod's workdir to rsync back after run.",
        ),
    },
)

# ============================================================
# runpod_job — bind a manifest to a pod_type + image, emit spec.
# ============================================================
def _runpod_job_impl(ctx):
    manifest_info = ctx.attr.manifest[RunpodManifestInfo]
    spec = ctx.actions.declare_file(ctx.label.name + ".jobspec.json")

    # Compose a small JSON spec the CLI's `train --spec=<path>`
    # entrypoint will read. v0.0.1 builds it inline via
    # ctx.actions.write; v0.0.2 promotes to a Rust spec-writer so
    # the schema is validated against the CLI's struct.
    spec_dict = {
        "name": ctx.label.name,
        "manifest_path": manifest_info.toml.short_path,
        "workdir": manifest_info.workdir,
        "outputs": manifest_info.outputs,
        "pod_type": ctx.attr.pod_type,
        "image": ctx.attr.image,
        "env": ctx.attr.env,
        "secrets": ctx.attr.secrets,
        "max_minutes": ctx.attr.max_minutes,
    }
    ctx.actions.write(
        output = spec,
        content = json.encode_indent(spec_dict),
    )

    return [
        DefaultInfo(files = depset([spec])),
        RunpodJobInfo(
            spec_json = spec,
            manifest = manifest_info,
            pod_type = ctx.attr.pod_type,
            image = ctx.attr.image,
        ),
    ]

runpod_job = rule(
    implementation = _runpod_job_impl,
    attrs = {
        "manifest": attr.label(
            mandatory = True,
            providers = [RunpodManifestInfo],
        ),
        "pod_type": attr.string(
            mandatory = True,
            doc = "RunPod GPU type id, e.g. \"NVIDIA H100 80GB HBM3\".",
        ),
        "image": attr.string(
            mandatory = True,
            doc = "Container image to run on the pod.",
        ),
        "env": attr.string_dict(
            doc = "Environment variables passed to setup + run scripts.",
        ),
        "secrets": attr.string_list(
            doc = "Secret names to inject from the local .env (RUNPOD_API_KEY, HF_TOKEN, ...).",
        ),
        "max_minutes": attr.int(
            default = 0,
            doc = "Wall-clock budget. 0 = no limit.",
        ),
    },
)

# ============================================================
# runpod_network_volume — declare a persistent network volume.
# ============================================================
#
# Volumes are named, persistent storage that outlive pods +
# serverless workers. The killer use case for serverless LLM
# hosting: cache the model weights on the volume so a cold worker
# mounts the model (seconds) instead of re-downloading tens of GB
# (minutes) on every spin-up.
#
# Emits a `NetworkVolumeCreateInput`-shaped JSON spec. The macro
# layer adds a `<name>.create` runner. Volumes are created once
# and referenced by many endpoints/pods (by name → id resolution
# at deploy time), so creation is decoupled from endpoint deploy.
def _runpod_network_volume_impl(ctx):
    spec = ctx.actions.declare_file(ctx.label.name + ".volume.json")
    volume_name = ctx.attr.volume_name or ctx.label.name
    spec_dict = {
        "name": volume_name,
        "size": ctx.attr.size_gb,
        "dataCenterId": ctx.attr.data_center_id,
    }
    ctx.actions.write(
        output = spec,
        content = json.encode_indent(spec_dict),
    )
    return [
        DefaultInfo(files = depset([spec])),
        RunpodNetworkVolumeInfo(
            spec_json = spec,
            volume_name = volume_name,
            size_gb = ctx.attr.size_gb,
            data_center_id = ctx.attr.data_center_id,
            mount_path = ctx.attr.mount_path,
        ),
    ]

runpod_network_volume = rule(
    implementation = _runpod_network_volume_impl,
    attrs = {
        "size_gb": attr.int(
            mandatory = True,
            doc = "Volume size in GB. Size it for the model(s) you'll cache " +
                  "(e.g. 100 for a 14B+ model cache with room to spare).",
        ),
        "data_center_id": attr.string(
            mandatory = True,
            doc = "RunPod data center id (e.g. 'US-CA-2', 'EU-RO-1'). " +
                  "Endpoints/pods referencing this volume MUST run in the " +
                  "same data center.",
        ),
        "volume_name": attr.string(
            default = "",
            doc = "Display name + deploy-time lookup key. Defaults to the target name.",
        ),
        "mount_path": attr.string(
            default = "/runpod-volume",
            doc = "Informational. Serverless mounts volumes at /runpod-volume; " +
                  "pods mount at the configured path.",
        ),
    },
)

# ============================================================
# runpod_pod — declare a long-lived service pod.
# ============================================================
def _runpod_pod_impl(ctx):
    spec = ctx.actions.declare_file(ctx.label.name + ".podspec.json")
    spec_dict = {
        "name": ctx.label.name,
        "image": ctx.attr.image,
        "pod_type": ctx.attr.pod_type,
        "ports": ctx.attr.ports,
        "env": ctx.attr.env,
        "secrets": ctx.attr.secrets,
        "container_disk_gb": ctx.attr.container_disk_gb,
        "volume_path": ctx.attr.volume_path,
    }
    ctx.actions.write(
        output = spec,
        content = json.encode_indent(spec_dict),
    )
    return [
        DefaultInfo(files = depset([spec])),
        RunpodPodInfo(
            spec_json = spec,
            pod_name = ctx.label.name,
            ports = ctx.attr.ports,
        ),
    ]

runpod_pod = rule(
    implementation = _runpod_pod_impl,
    attrs = {
        "image": attr.string(mandatory = True),
        "pod_type": attr.string(mandatory = True),
        "ports": attr.int_list(
            doc = "TCP ports the consumer will SSH-tunnel locally.",
        ),
        "env": attr.string_dict(),
        "secrets": attr.string_list(),
        "container_disk_gb": attr.int(default = 50),
        "volume_path": attr.string(
            default = "",
            doc = "Optional network-volume mount path (e.g. /workspace).",
        ),
    },
)

# ============================================================
# runpod_template — declare a reusable image+config bundle.
# ============================================================
#
# Emits a `TemplateCreateInput`-shaped JSON spec at build time;
# the macro layer (in defs.bzl) provides a `<name>.create`
# sh_binary that hands the spec to `runpod template
# create-from-spec`. Templates exist independently of pods /
# endpoints — register once, reference by template_id from many
# `runpod_pod` / `runpod_serverless_endpoint` targets.
def _runpod_template_impl(ctx):
    spec = ctx.actions.declare_file(ctx.label.name + ".template.json")

    # Serverless templates can't attach a network volume — the
    # RunPod REST API returns 500 if volumeInGb > 0 on a
    # serverless template. Force it to 0 (empty mount path) when
    # is_serverless; honor volume_gb / volume_path otherwise.
    volume_gb = 0 if ctx.attr.is_serverless else ctx.attr.volume_gb
    volume_path = "" if ctx.attr.is_serverless else ctx.attr.volume_path

    spec_dict = {
        "name": ctx.attr.template_name or ctx.label.name,
        "imageName": ctx.attr.image,
        "containerDiskInGb": ctx.attr.container_disk_gb,
        "env": ctx.attr.env,
        "ports": ctx.attr.ports,
        "isServerless": ctx.attr.is_serverless,
        "dockerStartCmd": ctx.attr.docker_start_cmd,
        "readme": ctx.attr.readme,
        "isPublic": False,
        "volumeInGb": volume_gb,
        "volumeMountPath": volume_path,
    }
    ctx.actions.write(
        output = spec,
        content = json.encode_indent(spec_dict),
    )
    return [
        DefaultInfo(files = depset([spec])),
        RunpodTemplateInfo(
            spec_json = spec,
            name = spec_dict["name"],
            image = ctx.attr.image,
            is_serverless = ctx.attr.is_serverless,
        ),
    ]

runpod_template = rule(
    implementation = _runpod_template_impl,
    attrs = {
        "image": attr.string(
            mandatory = True,
            doc = "Container image reference (e.g. 'vllm/vllm-openai:latest').",
        ),
        "template_name": attr.string(
            default = "",
            doc = "Display name. Defaults to the target name.",
        ),
        "container_disk_gb": attr.int(default = 50),
        "env": attr.string_dict(
            doc = "Environment variables exposed to the container.",
        ),
        "ports": attr.string_list(
            doc = "Port declarations, each formatted as '<port>/<proto>' (e.g. '8000/http').",
        ),
        "is_serverless": attr.bool(
            default = False,
            doc = "Mark the template serverless-only.",
        ),
        "docker_start_cmd": attr.string_list(
            doc = "Override the image's start command.",
        ),
        "readme": attr.string(default = ""),
        "volume_gb": attr.int(
            default = 20,
            doc = "Volume size for the pod attached to this template.",
        ),
        "volume_path": attr.string(
            default = "/workspace",
            doc = "Where the volume is mounted in the container.",
        ),
    },
)

# ============================================================
# runpod_serverless_endpoint — declare a serverless endpoint.
# ============================================================
#
# Emits an `EndpointCreateInput`-shaped JSON spec. The
# `templateId` field is left empty in the artifact — it's
# resolved at deploy time by reading the template's id (which
# RunPod assigns when the template is created). The macro layer
# composes `template.create` + `endpoint.create-from-spec` into
# a `<name>.deploy` sh_binary.
def _runpod_serverless_endpoint_impl(ctx):
    spec = ctx.actions.declare_file(ctx.label.name + ".endpoint.json")
    template_info = ctx.attr.template[RunpodTemplateInfo]
    if not template_info.is_serverless:
        fail(
            "runpod_serverless_endpoint(name={!r}): referenced template {!r} ".format(
                ctx.label.name,
                str(ctx.attr.template.label),
            ) + "must have is_serverless = True",
        )

    # A volume can be referenced two ways:
    #   * network_volume (label) — Bazel-declared volume; the
    #     deploy runner resolves its NAME → id at deploy time
    #     (the volume must be created first via :<vol>.create).
    #   * network_volume_id (string) — a pre-existing volume id.
    # The label form wins if both are set. The deploy runner reads
    # `networkVolumeName` from the spec and resolves it; if only
    # `networkVolumeId` is present it's used verbatim.
    network_volume_name = ""
    if ctx.attr.network_volume:
        vol_info = ctx.attr.network_volume[RunpodNetworkVolumeInfo]
        network_volume_name = vol_info.volume_name

        # The endpoint MUST run in the volume's data center; if the
        # author left data_center_ids empty, pin it to the volume's.
        if not ctx.attr.data_center_ids:
            data_center_ids = [vol_info.data_center_id]
        else:
            data_center_ids = ctx.attr.data_center_ids
    else:
        data_center_ids = ctx.attr.data_center_ids

    spec_dict = {
        "name": ctx.attr.endpoint_name or ctx.label.name,
        # templateId is filled in by `.deploy` after the template
        # is created and its id is known.
        "templateId": "<resolved-at-deploy-time>",
        "gpuTypeIds": ctx.attr.gpu_type_ids,
        "dataCenterIds": data_center_ids,
        "computeType": ctx.attr.compute_type,
        "workersMin": ctx.attr.min_workers,
        "workersMax": ctx.attr.max_workers,
        "idleTimeout": max(1, ctx.attr.idle_timeout_seconds),
        "gpuCount": ctx.attr.gpu_count,
        "scalerType": ctx.attr.scaler_type,
        "scalerValue": ctx.attr.scaler_value,
        "vcpuCount": ctx.attr.vcpu_count,
        "executionTimeoutMs": ctx.attr.execution_timeout_ms,
        "flashboot": ctx.attr.flashboot,
        "networkVolumeId": ctx.attr.network_volume_id,
        # Non-API field consumed by deploy_endpoint.sh to resolve a
        # Bazel-declared volume's id by name. Stripped before the
        # spec is POSTed.
        "networkVolumeName": network_volume_name,
    }
    ctx.actions.write(
        output = spec,
        content = json.encode_indent(spec_dict),
    )
    return [
        DefaultInfo(files = depset([spec])),
        RunpodServerlessEndpointInfo(
            spec_json = spec,
            name = spec_dict["name"],
            template = template_info,
            min_workers = ctx.attr.min_workers,
            max_workers = ctx.attr.max_workers,
        ),
    ]

runpod_serverless_endpoint = rule(
    implementation = _runpod_serverless_endpoint_impl,
    attrs = {
        "template": attr.label(
            mandatory = True,
            providers = [RunpodTemplateInfo],
            doc = "Label of a `runpod_template` target (must have is_serverless = True).",
        ),
        "endpoint_name": attr.string(
            default = "",
            doc = "Display name. Defaults to the target name.",
        ),
        "gpu_type_ids": attr.string_list(
            doc = "Preferred GPU type ids in order (e.g. 'NVIDIA A100 80GB PCIe').",
        ),
        "data_center_ids": attr.string_list(
            doc = "Allowed data centers. Empty = any.",
        ),
        "compute_type": attr.string(
            default = "GPU",
            values = ["GPU", "CPU"],
            doc = "GPU or CPU workers.",
        ),
        "min_workers": attr.int(
            default = 0,
            doc = "Minimum warm workers (0 = pure on-demand with cold starts).",
        ),
        "max_workers": attr.int(
            default = 3,
            doc = "Maximum concurrent workers.",
        ),
        "idle_timeout_seconds": attr.int(
            default = 5,
            doc = "Seconds of idleness before scale-down.",
        ),
        "gpu_count": attr.int(
            default = 1,
            doc = "GPUs per worker.",
        ),
        "scaler_type": attr.string(
            default = "QUEUE_DELAY",
            values = ["QUEUE_DELAY", "REQUEST_COUNT"],
        ),
        "scaler_value": attr.int(default = 4),
        "vcpu_count": attr.int(default = 2),
        "execution_timeout_ms": attr.int(
            default = 0,
            doc = "Max ms per individual request (0 = provider default).",
        ),
        "flashboot": attr.bool(default = False),
        "network_volume": attr.label(
            providers = [RunpodNetworkVolumeInfo],
            doc = "Optional `runpod_network_volume` target. The deploy " +
                  "runner resolves its name → id (the volume must be " +
                  "created first via :<vol>.create). When set and " +
                  "data_center_ids is empty, the endpoint is pinned to " +
                  "the volume's data center (they must match).",
        ),
        "network_volume_id": attr.string(
            default = "",
            doc = "Optional pre-existing network volume id (alternative " +
                  "to `network_volume`). Used verbatim.",
        ),
    },
)
