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
    _runpod_network_volume_rule = "runpod_network_volume",
    _runpod_pod = "runpod_pod",
    _runpod_serverless_endpoint_rule = "runpod_serverless_endpoint",
    _runpod_template_rule = "runpod_template",
)

runpod_manifest = _runpod_manifest
runpod_pod = _runpod_pod

def runpod_network_volume(name, visibility = None, **kwargs):
    """Declare a persistent network volume (model cache / dataset store).

    Emits two targets:
      * `<name>`         — the typed NetworkVolumeCreateInput JSON spec.
      * `<name>.create`  — sh_binary that creates (or reuses) the
                           volume via `runpod volume create-from-spec`
                           and prints its id.

    Volumes are created once and referenced by many endpoints/pods.
    Reference one from a `runpod_serverless_endpoint` /
    `runpod_inference_endpoint` via its `network_volume` attr — the
    deploy runner resolves the volume's name → id (so a serverless
    vLLM worker mounts the cached model instead of re-downloading it
    on every cold start).

    Args:
      name: target name (also the volume's display name + lookup key).
      visibility: standard.
      **kwargs: forwarded to the rule — `size_gb` (required),
        `data_center_id` (required), `volume_name`, `mount_path`.
    """
    _runpod_network_volume_rule(name = name, visibility = visibility, **kwargs)
    sh_binary(
        name = name + ".create",
        srcs = ["@rules_runpod//runpod:create_volume.sh"],
        data = [
            ":" + name,
            "@rules_runpod//:cli",
        ],
        args = [
            "$(rlocationpath @rules_runpod//:cli)",
            "$(rlocationpath :" + name + ")",
        ],
        deps = ["@bazel_tools//tools/bash/runfiles"],
        visibility = visibility,
    )

def runpod_template(name, visibility = None, **kwargs):
    """Declare a reusable template (image+env+ports+disk).

    Emits two targets:
      * `<name>`         — the typed TemplateCreateInput JSON spec.
      * `<name>.create`  — sh_binary that POSTs the spec via
                           `runpod template create-from-spec`.

    Args:
      name: target name.
      visibility: standard.
      **kwargs: forwarded to the `runpod_template` rule. See
        `runpod/private/rules.bzl` for the full attribute list
        (image, container_disk_gb, env, ports, is_serverless,
        docker_start_cmd, readme, volume_gb, volume_path).
    """
    _runpod_template_rule(name = name, visibility = visibility, **kwargs)
    sh_binary(
        name = name + ".create",
        srcs = ["@rules_runpod//runpod:create_template.sh"],
        data = [
            ":" + name,
            "@rules_runpod//:cli",
        ],
        args = [
            "$(rlocationpath @rules_runpod//:cli)",
            "$(rlocationpath :" + name + ")",
        ],
        deps = ["@bazel_tools//tools/bash/runfiles"],
        visibility = visibility,
    )

def runpod_serverless_endpoint(name, template, visibility = None, **kwargs):
    """Declare a serverless endpoint backed by a template.

    Emits two targets:
      * `<name>`         — the typed EndpointCreateInput JSON spec
                           (with `templateId` placeholder).
      * `<name>.deploy`  — sh_binary that, on `bazel run`:
                             1. POSTs the template spec → captures id
                             2. patches the endpoint spec
                             3. POSTs the endpoint spec → prints result

    Args:
      name: target name.
      template: label of a `runpod_template` target (must have
        is_serverless=True).
      visibility: standard.
      **kwargs: forwarded to the `runpod_serverless_endpoint`
        rule (gpu_type_ids, min_workers, max_workers,
        idle_timeout_seconds, ...).
    """
    _runpod_serverless_endpoint_rule(
        name = name,
        template = template,
        visibility = visibility,
        **kwargs
    )
    sh_binary(
        name = name + ".deploy",
        srcs = ["@rules_runpod//runpod:deploy_endpoint.sh"],
        data = [
            template,
            ":" + name,
            "@rules_runpod//:cli",
        ],
        args = [
            "$(rlocationpath @rules_runpod//:cli)",
            "$(rlocationpath %s)" % template,
            "$(rlocationpath :" + name + ")",
        ],
        deps = ["@bazel_tools//tools/bash/runfiles"],
        visibility = visibility,
    )

def runpod_inference_endpoint(
        name,
        model,
        gpu_type_ids,
        # RunPod SERVERLESS requires the worker image (implements the
        # queue handler + OpenAI-compatible routing), NOT the raw
        # `vllm/vllm-openai` server (that's for pods). The version
        # matters: `stable` is vLLM v1.x and crashes on newer model
        # architectures (e.g. Qwen2.5) — pin a recent v2.x.
        image = "runpod/worker-v1-vllm:v2.7.0stable-cuda12.1.0",
        max_model_len = 32768,
        min_workers = 0,
        max_workers = 3,
        idle_timeout_seconds = 5,
        data_center_ids = None,
        scaler_type = "QUEUE_DELAY",
        scaler_value = 4,
        env = None,
        container_disk_gb = 50,
        readme = "",
        flashboot = False,
        network_volume = None,
        hf_cache_path = "/runpod-volume/huggingface",
        visibility = None):
    """Convenience macro: declare a vLLM-served LLM as a serverless endpoint.

    Composes a `runpod_template` (image + model env + the
    OpenAI-compatible port) with a `runpod_serverless_endpoint`
    pointing at it. Produces three Bazel targets:

      `<name>_template`     — the typed template spec.
      `<name>`              — the typed endpoint spec.
      `<name>.deploy_spec`  — alias of the endpoint spec for
                              clarity in downstream `bazel build`s.

    Sized for the common case: vLLM image, OpenAI-compatible
    server on port 8000, a HuggingFace model id passed in via
    the `MODEL_NAME` env, `--max-model-len` from `max_model_len`.
    Override `image` / `env` for non-vLLM substrates (tgi,
    llama.cpp).

    Args:
      name: target name (also the endpoint display name).
      model: HuggingFace model id (e.g.
        'Qwen/Qwen2.5-Coder-32B-Instruct'). Passed to vLLM as
        `MODEL_NAME`.
      gpu_type_ids: preferred GPU type ids in order.
      image: container image. Defaults to the latest vLLM
        OpenAI-compatible server.
      max_model_len: vLLM `--max-model-len` setting.
      min_workers: minimum autoscaled workers (see `runpod_serverless_endpoint`).
      max_workers: maximum autoscaled workers.
      idle_timeout_seconds: seconds a worker idles before scale-down.
      data_center_ids: list of data centers to allow.
      scaler_type: scaling policy type (e.g. `QUEUE_DELAY`).
      scaler_value: scaling policy threshold.
      env: extra env vars to merge in beyond MODEL_NAME +
        MAX_MODEL_LEN.
      container_disk_gb: disk per worker.
      readme: README content for the template.
      flashboot: enable RunPod's flashboot feature.
      network_volume: optional `runpod_network_volume` target. When
        set, the model is cached on the volume (HF_HOME pointed at
        `hf_cache_path`), so cold workers mount the weights instead
        of re-downloading them — the fix for serverless LLM cold-start
        cost. The endpoint is pinned to the volume's data center.
        Create the volume first: `bazel run :<vol>.create`.
      hf_cache_path: where on the mounted volume HuggingFace caches
        models. Default `/runpod-volume/huggingface` (RunPod mounts
        serverless volumes at /runpod-volume). Only used when
        network_volume is set.
      visibility: standard.
    """
    merged_env = {
        "MODEL_NAME": model,
        "MAX_MODEL_LEN": str(max_model_len),
    }

    # When a model-cache volume is attached, point HuggingFace's
    # cache onto it so the weights persist across cold workers.
    if network_volume:
        merged_env["HF_HOME"] = hf_cache_path

    for k, v in (env or {}).items():
        merged_env[k] = v

    # Call the public macros so sub-targets pick up `.create` /
    # `.deploy` sh_binary runners automatically.
    runpod_template(
        name = name + "_template",
        template_name = name + "-template",
        image = image,
        container_disk_gb = container_disk_gb,
        env = merged_env,
        ports = ["8000/http"],
        is_serverless = True,
        readme = readme,
        visibility = visibility,
    )
    runpod_serverless_endpoint(
        name = name,
        template = ":" + name + "_template",
        endpoint_name = name,
        gpu_type_ids = gpu_type_ids,
        data_center_ids = data_center_ids or [],
        min_workers = min_workers,
        max_workers = max_workers,
        idle_timeout_seconds = idle_timeout_seconds,
        scaler_type = scaler_type,
        scaler_value = scaler_value,
        flashboot = flashboot,
        network_volume = network_volume,
        visibility = visibility,
    )
    # The endpoint macro's `<name>.deploy` IS the entry point — that
    # single `bazel run` provisions both the template and the
    # endpoint. Expose it under the inference-endpoint's name too
    # as a friendlier alias so consumers can `bazel run :foo.deploy`
    # without remembering that the inference macro layers two
    # underlying targets.

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
