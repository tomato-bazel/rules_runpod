<!-- Generated with Stardoc: http://skydoc.bazel.build -->

rules_runpod public API.

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

<a id="runpod_manifest"></a>

## runpod_manifest

<pre>
load("@rules_runpod//runpod:defs.bzl", "runpod_manifest")

runpod_manifest(<a href="#runpod_manifest-name">name</a>, <a href="#runpod_manifest-src">src</a>, <a href="#runpod_manifest-outputs">outputs</a>, <a href="#runpod_manifest-workdir">workdir</a>)
</pre>



**ATTRIBUTES**


| Name  | Description | Type | Mandatory | Default |
| :------------- | :------------- | :------------- | :------------- | :------------- |
| <a id="runpod_manifest-name"></a>name |  A unique name for this target.   | <a href="https://bazel.build/concepts/labels#target-names">Name</a> | required |  |
| <a id="runpod_manifest-src"></a>src |  Path to the manifest TOML.   | <a href="https://bazel.build/concepts/labels">Label</a> | required |  |
| <a id="runpod_manifest-outputs"></a>outputs |  Paths under the pod's workdir to rsync back after run.   | List of strings | optional |  `[]`  |
| <a id="runpod_manifest-workdir"></a>workdir |  Workspace-relative path to rsync to the pod.   | String | optional |  `"."`  |


<a id="runpod_pod"></a>

## runpod_pod

<pre>
load("@rules_runpod//runpod:defs.bzl", "runpod_pod")

runpod_pod(<a href="#runpod_pod-name">name</a>, <a href="#runpod_pod-container_disk_gb">container_disk_gb</a>, <a href="#runpod_pod-env">env</a>, <a href="#runpod_pod-image">image</a>, <a href="#runpod_pod-pod_type">pod_type</a>, <a href="#runpod_pod-ports">ports</a>, <a href="#runpod_pod-secrets">secrets</a>, <a href="#runpod_pod-volume_path">volume_path</a>)
</pre>



**ATTRIBUTES**


| Name  | Description | Type | Mandatory | Default |
| :------------- | :------------- | :------------- | :------------- | :------------- |
| <a id="runpod_pod-name"></a>name |  A unique name for this target.   | <a href="https://bazel.build/concepts/labels#target-names">Name</a> | required |  |
| <a id="runpod_pod-container_disk_gb"></a>container_disk_gb |  -   | Integer | optional |  `50`  |
| <a id="runpod_pod-env"></a>env |  -   | <a href="https://bazel.build/rules/lib/dict">Dictionary: String -> String</a> | optional |  `{}`  |
| <a id="runpod_pod-image"></a>image |  -   | String | required |  |
| <a id="runpod_pod-pod_type"></a>pod_type |  -   | String | required |  |
| <a id="runpod_pod-ports"></a>ports |  TCP ports the consumer will SSH-tunnel locally.   | List of integers | optional |  `[]`  |
| <a id="runpod_pod-secrets"></a>secrets |  -   | List of strings | optional |  `[]`  |
| <a id="runpod_pod-volume_path"></a>volume_path |  Optional network-volume mount path (e.g. /workspace).   | String | optional |  `""`  |


<a id="runpod_inference_endpoint"></a>

## runpod_inference_endpoint

<pre>
load("@rules_runpod//runpod:defs.bzl", "runpod_inference_endpoint")

runpod_inference_endpoint(<a href="#runpod_inference_endpoint-name">name</a>, <a href="#runpod_inference_endpoint-model">model</a>, <a href="#runpod_inference_endpoint-gpu_type_ids">gpu_type_ids</a>, <a href="#runpod_inference_endpoint-image">image</a>, <a href="#runpod_inference_endpoint-max_model_len">max_model_len</a>, <a href="#runpod_inference_endpoint-min_workers">min_workers</a>, <a href="#runpod_inference_endpoint-max_workers">max_workers</a>,
                          <a href="#runpod_inference_endpoint-idle_timeout_seconds">idle_timeout_seconds</a>, <a href="#runpod_inference_endpoint-data_center_ids">data_center_ids</a>, <a href="#runpod_inference_endpoint-scaler_type">scaler_type</a>, <a href="#runpod_inference_endpoint-scaler_value">scaler_value</a>, <a href="#runpod_inference_endpoint-env">env</a>,
                          <a href="#runpod_inference_endpoint-container_disk_gb">container_disk_gb</a>, <a href="#runpod_inference_endpoint-readme">readme</a>, <a href="#runpod_inference_endpoint-flashboot">flashboot</a>, <a href="#runpod_inference_endpoint-network_volume">network_volume</a>, <a href="#runpod_inference_endpoint-hf_cache_path">hf_cache_path</a>,
                          <a href="#runpod_inference_endpoint-visibility">visibility</a>)
</pre>

Convenience macro: declare a vLLM-served LLM as a serverless endpoint.

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


**PARAMETERS**


| Name  | Description | Default Value |
| :------------- | :------------- | :------------- |
| <a id="runpod_inference_endpoint-name"></a>name |  target name (also the endpoint display name).   |  none |
| <a id="runpod_inference_endpoint-model"></a>model |  HuggingFace model id (e.g. 'Qwen/Qwen2.5-Coder-32B-Instruct'). Passed to vLLM as `MODEL_NAME`.   |  none |
| <a id="runpod_inference_endpoint-gpu_type_ids"></a>gpu_type_ids |  preferred GPU type ids in order.   |  none |
| <a id="runpod_inference_endpoint-image"></a>image |  container image. Defaults to the latest vLLM OpenAI-compatible server.   |  `"runpod/worker-v1-vllm:v2.7.0stable-cuda12.1.0"` |
| <a id="runpod_inference_endpoint-max_model_len"></a>max_model_len |  vLLM `--max-model-len` setting.   |  `32768` |
| <a id="runpod_inference_endpoint-min_workers"></a>min_workers |  minimum autoscaled workers (see `runpod_serverless_endpoint`).   |  `0` |
| <a id="runpod_inference_endpoint-max_workers"></a>max_workers |  maximum autoscaled workers.   |  `3` |
| <a id="runpod_inference_endpoint-idle_timeout_seconds"></a>idle_timeout_seconds |  seconds a worker idles before scale-down.   |  `5` |
| <a id="runpod_inference_endpoint-data_center_ids"></a>data_center_ids |  list of data centers to allow.   |  `None` |
| <a id="runpod_inference_endpoint-scaler_type"></a>scaler_type |  scaling policy type (e.g. `QUEUE_DELAY`).   |  `"QUEUE_DELAY"` |
| <a id="runpod_inference_endpoint-scaler_value"></a>scaler_value |  scaling policy threshold.   |  `4` |
| <a id="runpod_inference_endpoint-env"></a>env |  extra env vars to merge in beyond MODEL_NAME + MAX_MODEL_LEN.   |  `None` |
| <a id="runpod_inference_endpoint-container_disk_gb"></a>container_disk_gb |  disk per worker.   |  `50` |
| <a id="runpod_inference_endpoint-readme"></a>readme |  README content for the template.   |  `""` |
| <a id="runpod_inference_endpoint-flashboot"></a>flashboot |  enable RunPod's flashboot feature.   |  `False` |
| <a id="runpod_inference_endpoint-network_volume"></a>network_volume |  optional `runpod_network_volume` target. When set, the model is cached on the volume (HF_HOME pointed at `hf_cache_path`), so cold workers mount the weights instead of re-downloading them — the fix for serverless LLM cold-start cost. The endpoint is pinned to the volume's data center. Create the volume first: `bazel run :<vol>.create`.   |  `None` |
| <a id="runpod_inference_endpoint-hf_cache_path"></a>hf_cache_path |  where on the mounted volume HuggingFace caches models. Default `/runpod-volume/huggingface` (RunPod mounts serverless volumes at /runpod-volume). Only used when network_volume is set.   |  `"/runpod-volume/huggingface"` |
| <a id="runpod_inference_endpoint-visibility"></a>visibility |  standard.   |  `None` |


<a id="runpod_job"></a>

## runpod_job

<pre>
load("@rules_runpod//runpod:defs.bzl", "runpod_job")

runpod_job(<a href="#runpod_job-name">name</a>, <a href="#runpod_job-manifest">manifest</a>, <a href="#runpod_job-pod_type">pod_type</a>, <a href="#runpod_job-image">image</a>, <a href="#runpod_job-env">env</a>, <a href="#runpod_job-secrets">secrets</a>, <a href="#runpod_job-max_minutes">max_minutes</a>, <a href="#runpod_job-ephemeral">ephemeral</a>, <a href="#runpod_job-visibility">visibility</a>)
</pre>

Declare a one-shot RunPod training job.

Emits two targets:
  * `<name>`     — the typed jobspec.json (build artifact).
  * `<name>.run` — a sh_binary that, on `bazel run`, invokes
                   `runpod-cli train <manifest>` against the
                   active workspace. Forwards extra args
                   (`bazel run :<name>.run -- --debug`).


**PARAMETERS**


| Name  | Description | Default Value |
| :------------- | :------------- | :------------- |
| <a id="runpod_job-name"></a>name |  target name.   |  none |
| <a id="runpod_job-manifest"></a>manifest |  label to a `runpod_manifest` target.   |  none |
| <a id="runpod_job-pod_type"></a>pod_type |  GPU type string (e.g., `NVIDIA A100 80GB PCIe`).   |  none |
| <a id="runpod_job-image"></a>image |  container image.   |  none |
| <a id="runpod_job-env"></a>env |  extra env vars (dict).   |  `None` |
| <a id="runpod_job-secrets"></a>secrets |  env var names to forward from the local shell.   |  `None` |
| <a id="runpod_job-max_minutes"></a>max_minutes |  ceiling on the train run (0 = no cap).   |  `0` |
| <a id="runpod_job-ephemeral"></a>ephemeral |  when True, the `.run` wrapper passes both `--down-on-success` and `--down-on-failure` to runpod-cli, so the pod is torn down regardless of whether the run succeeded — orphan-free iteration. Default False keeps the legacy debug-friendly behavior (pod stays running on failure so the operator can SSH in).   |  `False` |
| <a id="runpod_job-visibility"></a>visibility |  standard.   |  `None` |


<a id="runpod_network_volume"></a>

## runpod_network_volume

<pre>
load("@rules_runpod//runpod:defs.bzl", "runpod_network_volume")

runpod_network_volume(<a href="#runpod_network_volume-name">name</a>, <a href="#runpod_network_volume-visibility">visibility</a>, <a href="#runpod_network_volume-kwargs">**kwargs</a>)
</pre>

Declare a persistent network volume (model cache / dataset store).

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


**PARAMETERS**


| Name  | Description | Default Value |
| :------------- | :------------- | :------------- |
| <a id="runpod_network_volume-name"></a>name |  target name (also the volume's display name + lookup key).   |  none |
| <a id="runpod_network_volume-visibility"></a>visibility |  standard.   |  `None` |
| <a id="runpod_network_volume-kwargs"></a>kwargs |  forwarded to the rule — `size_gb` (required), `data_center_id` (required), `volume_name`, `mount_path`.   |  none |


<a id="runpod_serverless_endpoint"></a>

## runpod_serverless_endpoint

<pre>
load("@rules_runpod//runpod:defs.bzl", "runpod_serverless_endpoint")

runpod_serverless_endpoint(<a href="#runpod_serverless_endpoint-name">name</a>, <a href="#runpod_serverless_endpoint-template">template</a>, <a href="#runpod_serverless_endpoint-visibility">visibility</a>, <a href="#runpod_serverless_endpoint-kwargs">**kwargs</a>)
</pre>

Declare a serverless endpoint backed by a template.

Emits two targets:
  * `<name>`         — the typed EndpointCreateInput JSON spec
                       (with `templateId` placeholder).
  * `<name>.deploy`  — sh_binary that, on `bazel run`:
                         1. POSTs the template spec → captures id
                         2. patches the endpoint spec
                         3. POSTs the endpoint spec → prints result


**PARAMETERS**


| Name  | Description | Default Value |
| :------------- | :------------- | :------------- |
| <a id="runpod_serverless_endpoint-name"></a>name |  target name.   |  none |
| <a id="runpod_serverless_endpoint-template"></a>template |  label of a `runpod_template` target (must have is_serverless=True).   |  none |
| <a id="runpod_serverless_endpoint-visibility"></a>visibility |  standard.   |  `None` |
| <a id="runpod_serverless_endpoint-kwargs"></a>kwargs |  forwarded to the `runpod_serverless_endpoint` rule (gpu_type_ids, min_workers, max_workers, idle_timeout_seconds, ...).   |  none |


<a id="runpod_template"></a>

## runpod_template

<pre>
load("@rules_runpod//runpod:defs.bzl", "runpod_template")

runpod_template(<a href="#runpod_template-name">name</a>, <a href="#runpod_template-visibility">visibility</a>, <a href="#runpod_template-kwargs">**kwargs</a>)
</pre>

Declare a reusable template (image+env+ports+disk).

Emits two targets:
  * `<name>`         — the typed TemplateCreateInput JSON spec.
  * `<name>.create`  — sh_binary that POSTs the spec via
                       `runpod template create-from-spec`.


**PARAMETERS**


| Name  | Description | Default Value |
| :------------- | :------------- | :------------- |
| <a id="runpod_template-name"></a>name |  target name.   |  none |
| <a id="runpod_template-visibility"></a>visibility |  standard.   |  `None` |
| <a id="runpod_template-kwargs"></a>kwargs |  forwarded to the `runpod_template` rule. See `runpod/private/rules.bzl` for the full attribute list (image, container_disk_gb, env, ports, is_serverless, docker_start_cmd, readme, volume_gb, volume_path).   |  none |


