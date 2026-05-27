"""rules_runpod private rule implementations.

The macros in //runpod:defs.bzl are thin wrappers around these
rules. Public surface is the macros; rule shapes can evolve.

The shape mirrors `rules_lora`: build-time emits a typed JSON
spec; execute-time is the `runpod-cli` consuming the spec.
"""

load(":providers.bzl",
     "RunpodJobInfo",
     "RunpodManifestInfo",
     "RunpodPodInfo")

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
