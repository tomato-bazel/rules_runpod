#!/usr/bin/env bash
# runpod_job.run — invoke `runpod-cli train <manifest>` against the
# manifest declared by the `runpod_job` rule. `bazel run
# //path:<job_name>.run` lands here.
#
# Args (injected by the macro via $(execpath ...)):
#   $1 — path to the runpod-cli binary (runfiles-relative)
#   $2 — path to the jobspec.json     (runfiles-relative)
#   $3 — path to the manifest TOML    (runfiles-relative)
#
# Behavior:
#   * Resolve all three to absolute paths *before* `cd`-ing, so they
#     remain valid after we leave the runfiles tree.
#   * `cd` to BUILD_WORKSPACE_DIRECTORY so the manifest's
#     `workdir = "."` resolves against the user's checkout.
#   * Forward any further `bazel run -- <extra args>` to the CLI.
#
# Note: the manifest path is now a build-time-injected runfile rather
# than read from the jobspec's `manifest_path` field. This makes the
# wrapper work uniformly for source-tree manifests and `bazel-bin`
# generated manifests (e.g. those synthesized by
# `@rules_lora//lora:lora_train`).

set -euo pipefail

if [[ $# -lt 3 ]]; then
    echo "fatal: runner.sh requires CLI_PATH, SPEC_PATH, and MANIFEST_PATH" >&2
    exit 2
fi

CLI_PATH="$(realpath "$1")"
SPEC_PATH="$(realpath "$2")"
MANIFEST_PATH="$(realpath "$3")"
shift 3

# `bazel run` sets BUILD_WORKSPACE_DIRECTORY to the workspace root.
# Outside `bazel run` (rare) fall back to PWD so the script is still
# debuggable.
cd "${BUILD_WORKSPACE_DIRECTORY:-$(pwd)}"

echo "runpod_job.run: $CLI_PATH train --manifest $MANIFEST_PATH $*" >&2
echo "  (jobspec metadata at: $SPEC_PATH)" >&2
exec "$CLI_PATH" train --manifest "$MANIFEST_PATH" "$@"
