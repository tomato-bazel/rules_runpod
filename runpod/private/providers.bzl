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

RunpodTemplateInfo = provider(
    doc = "A RunPod template spec — image + env + ports + disk.",
    fields = {
        "spec_json": "File — the rendered TemplateCreateInput JSON.",
        "name": "str — template display name (also the consumer's reference).",
        "image": "str — container image reference.",
        "is_serverless": "bool — true if this template is serverless-only.",
    },
)

RunpodServerlessEndpointInfo = provider(
    doc = "A serverless endpoint spec — references a template.",
    fields = {
        "spec_json": "File — the rendered EndpointCreateInput JSON (template_id slot left empty; resolved at deploy time).",
        "name": "str — endpoint display name.",
        "template": "RunpodTemplateInfo of the template this endpoint serves.",
        "min_workers": "int — minimum warm workers.",
        "max_workers": "int — maximum concurrent workers.",
    },
)
