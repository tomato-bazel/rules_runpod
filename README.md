# rules_runpod

Bazel-native orchestration of [RunPod](https://www.runpod.io) GPU pods.

- **A Rust CLI + TUI** (`//:cli`, `//:tui`) over the RunPod REST API: pod
  lifecycle, manifest-driven training runs, SSH transport with detached
  execution, log streaming, templates, serverless endpoints and network
  volumes (including an S3-compatible data path).
- **Typed Starlark macros** (`@rules_runpod//runpod:defs.bzl`) so a repo can
  declare RunPod workloads as ordinary build targets and launch them with
  `bazel run`.

The RunPod API client is generated from RunPod's published OpenAPI spec
(`runpod-openapi.json`, patched by `patch_spec.py`) via
[rules_openapi](https://github.com/tomato-bazel/rules_openapi).

## Setup

`rules_runpod` is published to the
[tomato-bazel registry](https://github.com/tomato-bazel/bazel-registry).
Add it ahead of the BCR in your `.bazelrc`:

```
common --registry=https://registry.tbzl.dev/
common --registry=https://bcr.bazel.build/

# rules_runpod declares an isolated crate universe (`use_extension(...,
# isolate = True)`); Bazel rejects that without this flag.
common --experimental_isolated_extension_usages
```

Then in `MODULE.bazel`:

```starlark
bazel_dep(name = "rules_runpod", version = "0.1.0")
```

### Credentials

The CLI reads the RunPod API key from, in order:

1. the `RUNPOD_API_KEY` environment variable (a `.env` file in the working
   directory is loaded too);
2. `api_key` in the CLI config file — `~/Library/Application Support/runpod-cli/config.toml`
   on macOS, `~/.config/runpod-cli/config.toml` on Linux.

The same file may set `ssh_key_path`; otherwise `~/.ssh/id_ed25519` (or
`id_rsa`) is used. Network-volume S3 transfers use the standard `AWS_*`
environment variables holding RunPod S3 API keys.

## Usage

```starlark
load("@rules_runpod//runpod:defs.bzl",
     "runpod_manifest", "runpod_job", "runpod_pod")

# A TOML manifest (workdir, setup/run scripts, resources, envs) as a target.
runpod_manifest(
    name = "train",
    src = "runpod.toml",
    workdir = ".",
    outputs = ["runs"],
)

# One-shot job: `bazel run //path:train_job.run` provisions a pod, syncs the
# workdir, runs setup + run, streams logs, pulls outputs back, terminates.
runpod_job(
    name = "train_job",
    manifest = ":train",
    pod_type = "NVIDIA H100 80GB HBM3",
    image = "runpod/pytorch:2.5.1-py3.11-cuda12.4.1-devel-ubuntu22.04",
    ephemeral = True,  # tear the pod down on success *and* failure
)

# Long-lived service pod.
runpod_pod(
    name = "embed_server",
    image = "...",
    pod_type = "...",
    ports = [50063],
    secrets = ["HF_TOKEN"],
)
```

`runpod/defs.bzl` also provides `runpod_template`,
`runpod_serverless_endpoint`, `runpod_inference_endpoint` (vLLM in one
declaration) and `runpod_network_volume`. See [docs/defs.md](docs/defs.md)
for the full reference and [examples/](examples/) for working targets.

The CLI can also be used directly:

```
bazel run @rules_runpod//:cli -- --help
bazel run @rules_runpod//:tui
```

## Layout

```
api/                 bearer-auth wrapper over the generated RunPod client
cli/                 the `runpod-cli` binary
tui/                 ratatui interactive surface
tui-kit/             reusable ratatui widget library
runpod/              public macros (defs.bzl) + rule implementations
examples/            smoke + inference targets
runpod-openapi.json  vendored upstream API schema
patch_spec.py        post-processing of the upstream spec before codegen
```

## License

MIT — see [LICENSE](LICENSE).
