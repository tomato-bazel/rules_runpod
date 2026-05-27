# Changelog

All notable changes to rules_runpod. The format is loosely
[Keep a Changelog](https://keepachangelog.com/) — version headers
mirror the published bazel-registry entries (when published; v0
is premium / private).

## 0.0.3 — `<name>.run` wrapper accepts a bazel-bin manifest path

Fixes a class of failure that surfaced when `rules_lora` 0.0.4
started synthesizing the manifest TOML at build time: the
generated TOML landed in `bazel-bin/...`, but the v0.0.2 `runner.sh`
read `manifest_path` out of the jobspec JSON and tried to resolve
it against `$BUILD_WORKSPACE_DIRECTORY` (the source tree), where
nothing existed.

The runpod_job macro now bundles the manifest itself as a runfile
of the emitted `<name>.run` sh_binary and passes its `$(execpath
...)` as a third positional arg. runner.sh resolves all three
positional args (cli, jobspec, manifest) to absolute paths via
`realpath` *before* cd-ing to the workspace, so the manifest stays
reachable whether it's a source file or a bazel-bin artifact.

Bonus drive-by: config.rs now reports the resolved config-file
path in the "RUNPOD_API_KEY not set" error message (the v0.0.2
message named the Linux-XDG path even on macOS where the actual
file lives under `~/Library/Application Support/`).

## 0.0.2 — `runpod_job` auto-emits `<name>.run` sh_binary

The `runpod_job` macro now emits a sibling `<name>.run` sh_binary
alongside the typed jobspec target. Downstream consumers can:

    bazel run //path:my_job.run [-- <extra args forwarded to CLI>]

…to trigger the actual RunPod training run from the build-time
manifest binding. The wrapper reads `manifest_path` out of the
emitted `<name>.jobspec.json` (via python3) and invokes
`runpod-cli train --manifest <manifest>` from
`$BUILD_WORKSPACE_DIRECTORY` so relative manifest paths resolve.

## 0.0.1 — initial lift + public macros

Rust workspace lifted verbatim from
`prime-transformer/tools/runpod-cli`:

* `api/` — bearer-token wrapper around the progenitor-generated
  `runpod_api` (top-level `openapi_rust_client` target consumes
  the vendored `runpod-openapi.json`, patched by `patch_spec.py`).
* `cli/` — the `runpod-cli` binary (subcommands: `train`, `pod`,
  `ssh`, `exec`, `pull`, `status`, `terminate`, `restream`,
  `template`, `volume`).
* `tui/` — ratatui-based interactive surface over the API.
* `tui-kit/` — generic ratatui widget library; the base pattern
  for any future fastverk TUI app.

Public Starlark surface (`@rules_runpod//runpod:defs.bzl`):

* `runpod_manifest` — wrap a TOML manifest as a typed Bazel
  target (`RunpodManifestInfo` provider).
* `runpod_job` — bind manifest + pod_type + image into a typed
  job spec; emits `<name>.jobspec.json`. v0.0.2 adds the
  `bazel run :<name>` wrapper that invokes the CLI.
* `runpod_pod` — declare a long-lived service pod (klad-embed,
  Ollama, agora bidder); emits `<name>.podspec.json`.

Smoke at `examples/smoke/` exercises all three macros end-to-end:

    bazel build //examples/smoke:smoke_job //examples/smoke:smoke_service

emits two self-contained spec files with reproducible content.

All 13 targets in `bazel build //...` green.

Migration path for prime-transformer (next-turn commit there):
delete `tools/runpod-cli/` and switch `bazel_dep(rules_runpod)`,
re-targeting `runpod-{cli,tui}` consumers to
`@rules_runpod//:cli` and `@rules_runpod//:tui`.
