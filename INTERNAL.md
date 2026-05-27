# rules_runpod — internal notes

**Status:** private / fastverk premium tier. Do not push to GitHub,
do not register in any public bazel-registry.

## What this is

Bazel-native orchestration of [RunPod](https://www.runpod.io) GPU
pods. Three things in one repo:

1. **A Rust CLI + TUI** (`api/`, `cli/`, `tui/`, `tui-kit/`) lifted
   verbatim from prime-transformer's `tools/runpod-cli/`. Handles
   pod lifecycle, manifest-driven training runs, ssh transport,
   stdout streaming, and an interactive curses UI for status.
2. **A typed Starlark surface** (`runpod/defs.bzl`) — `runpod_manifest`,
   `runpod_job`, `runpod_pod` macros that let downstream repos
   declare RunPod workloads as first-class build targets.
3. **A toolchain** (`runpod/private/toolchain.bzl`) registering
   the Rust CLI so consumers can `_runpod_cli` it in their own
   action rules.

## Consumers

| repo | use |
|---|---|
| `[[prime-transformer]]` | training runs (next_prime_*, hp_sigma_drift, …). v0.0.1 still depends on the bundled `tools/runpod-cli`; migration to `@rules_runpod//runpod:cli` is the lift's first downstream PR. |
| `[[rules_lora]]` | `lora_train(backend = "runpod")` invokes `runpod_job` under the hood. `rules_lora/runtime/runpod_orchestrator/` becomes a thin wrapper around the typed manifest contract. |
| `[[klad]]` | `klad-embed` runs on a long-lived H100 pod via `runpod_pod` (TUI: status/logs/restart). |
| `[[agora]]` | bid-function pods (per-tool LoRAs serving at agora-bidder ports) declared as `runpod_pod` targets. |

## Public API

```starlark
load("@rules_runpod//runpod:defs.bzl",
     "runpod_manifest", "runpod_job", "runpod_pod")

# Declare a manifest as a Bazel target — typed, blake3-pinned.
runpod_manifest(
    name = "smoke",
    src = "runpod-smoke.toml",
    workdir = ".",
    outputs = ["runs"],
)

# Run a one-shot job: `bazel run //path:smoke_job` provisions a
# pod, syncs the workdir, runs setup+run, streams logs, optionally
# pulls outputs back, terminates. Mirrors `runpod-cli train`.
runpod_job(
    name = "smoke_job",
    manifest = ":smoke",
    pod_type = "NVIDIA H100 80GB HBM3",
    image = "runpod/pytorch:2.5.1-py3.11-cuda12.4.1-devel-ubuntu22.04",
)

# Long-lived service pod (gRPC / HTTP); idempotent
# `bazel run :klad_embed.start` brings it up, `:klad_embed.stop`
# tears it down.
runpod_pod(
    name = "klad_embed",
    image = "...",
    pod_type = "...",
    ports = [50063],
    secrets = ["HF_TOKEN"],
)
```

## Layout

```
api/                 progenitor-generated runpod-openapi client (Rust)
cli/                 the `runpod` binary — `train`, `pod`, `ssh`, `exec`, `pull`, …
tui/                 ratatui-based interactive surface
tui-kit/             reusable ratatui widget library
runpod/
  defs.bzl           public macros — runpod_manifest / _job / _pod
  private/
    rules.bzl        rule implementations
    toolchain.bzl    rust_binary toolchain registration
proto/
  runpod.proto       wire schema — RunpodJobSpec, RunpodPodSpec (mirrors lora.v1 shape)
examples/
  smoke/             a minimum-viable manifest + job target
runpod-openapi.json  upstream API schema (versioned)
patch_spec.py        post-processing of upstream OpenAPI (snake_case, etc.)
```

## Status

v0.0.1 lift: Rust crates intact, `bazel build //cli:runpod` green
end-to-end. Starlark rules in `runpod/defs.bzl` ship with macro
signatures stable. The macros' rule implementations write a
typed `RunpodJobSpec` JSON at build time (mirrors the
`lora.v1.TrainingJobSpec` pattern from rules_lora); execution
side at `bazel run` time is the CLI consuming the spec.

## Non-goals (v0)

* Multi-cloud abstraction (RunPod only; CoreWeave, Lambda, etc.
  would be separate `rules_*` repos behind a common
  `rules_gpu_pool` interface in v1+).
* Hermetic GPU compute. Training itself happens on the remote pod
  outside Bazel's sandbox — same pattern rules_tectonic uses for
  external tools, same pattern rules_lora's runpod backend uses.
