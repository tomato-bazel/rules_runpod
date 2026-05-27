"""rules_runpod Starlark providers.

Three typed surfaces, mirroring the wire-format shape in
`proto/runpod.proto`.
"""

RunpodManifestInfo = provider(
    doc = "A validated, sha-pinned RunPod manifest.",
    fields = {
        "toml": "File — the manifest TOML.",
        "sha": "str — BLAKE3 hex of the TOML bytes.",
        "workdir": "str — relative workspace path to upload to the pod.",
        "outputs": "list[str] — paths to rsync back after run.",
    },
)

RunpodJobInfo = provider(
    doc = "A composed RunPod job spec (manifest + pod-side knobs).",
    fields = {
        "spec_json": "File — the rendered RunpodJobSpec JSON.",
        "manifest": "RunpodManifestInfo of the manifest this spec wraps.",
        "pod_type": "str — RunPod GPU type identifier.",
        "image": "str — container image for the pod.",
    },
)

RunpodPodInfo = provider(
    doc = "A long-lived pod target — start/stop/restart actions.",
    fields = {
        "spec_json": "File — the rendered RunpodPodSpec JSON.",
        "pod_name": "str — stable pod identifier (deterministic).",
        "ports": "list[int] — exposed TCP ports for ssh tunnels.",
    },
)
