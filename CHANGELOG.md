# Changelog

All notable changes to rules_runpod. The format is loosely
[Keep a Changelog](https://keepachangelog.com/) — version headers
mirror the published bazel-registry entries (when published; v0
is premium / private).

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
