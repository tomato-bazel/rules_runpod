"""rules_runpod public API.

```starlark
load("@rules_runpod//runpod:defs.bzl",
     "runpod_manifest", "runpod_job", "runpod_pod")
```

* `runpod_manifest` — typed wrapper around a TOML manifest file.
* `runpod_job` — manifest + pod-type + image → typed jobspec.
                 v0.0.2: the macro also emits a `:<name>.run`
                 sh_binary that `bazel run :<name>.run` invokes
                 to launch the job via the CLI.
* `runpod_pod` — long-lived service-pod target. v0.0.3 will gain
                 `.start` / `.stop` / `.logs` runnable siblings.
"""

load("@rules_shell//shell:sh_binary.bzl", "sh_binary")
load(
    "//runpod/private:rules.bzl",
    _runpod_job_rule = "runpod_job",
    _runpod_manifest = "runpod_manifest",
    _runpod_pod = "runpod_pod",
)

runpod_manifest = _runpod_manifest
runpod_pod = _runpod_pod

def runpod_job(
        name,
        manifest,
        pod_type,
        image,
        env = None,
        secrets = None,
        max_minutes = 0,
        ephemeral = False,
        visibility = None):
    """Declare a one-shot RunPod training job.

    Emits two targets:
      * `<name>`     — the typed jobspec.json (build artifact).
      * `<name>.run` — a sh_binary that, on `bazel run`, invokes
                       `runpod-cli train <manifest>` against the
                       active workspace. Forwards extra args
                       (`bazel run :<name>.run -- --debug`).

    Args:
      name: target name.
      manifest: label to a `runpod_manifest` target.
      pod_type: GPU type string (e.g., `NVIDIA A100 80GB PCIe`).
      image: container image.
      env: extra env vars (dict).
      secrets: env var names to forward from the local shell.
      max_minutes: ceiling on the train run (0 = no cap).
      ephemeral: when True, the `.run` wrapper passes both
        `--down-on-success` and `--down-on-failure` to runpod-cli,
        so the pod is torn down regardless of whether the run
        succeeded — orphan-free iteration. Default False keeps
        the legacy debug-friendly behavior (pod stays running on
        failure so the operator can SSH in).
      visibility: standard.
    """
    _runpod_job_rule(
        name = name,
        manifest = manifest,
        pod_type = pod_type,
        image = image,
        env = env or {},
        secrets = secrets or [],
        max_minutes = max_minutes,
        visibility = visibility,
    )
    extra_args = []
    if ephemeral:
        extra_args = ["--down-on-success", "--down-on-failure"]
    sh_binary(
        name = name + ".run",
        srcs = ["@rules_runpod//runpod:runner.sh"],
        # Bundle the manifest itself so the runner gets an absolute
        # runfiles path to the TOML — works for both source-tree and
        # bazel-bin manifests (e.g. synthesized by lora_train).
        data = [
            ":" + name,
            manifest,
            "@rules_runpod//:cli",
        ],
        args = [
            "$(rlocationpath @rules_runpod//:cli)",
            "$(rlocationpath :" + name + ")",
            "$(rlocationpath %s)" % manifest,
        ] + extra_args,
        # rules_bash runfiles helper used by runner.sh's `rlocation`.
        deps = ["@bazel_tools//tools/bash/runfiles"],
        visibility = visibility,
    )
