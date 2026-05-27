#!/usr/bin/env bash
# runpod_job.run — invoke `runpod-cli train <manifest>` against the
# manifest referenced by the typed jobspec emitted by the
# `runpod_job` rule. `bazel run //path:<job_name>.run` lands here.
#
# Args (injected by the macro via $(execpath ...)):
#   $1 — absolute path to the runpod-cli binary
#   $2 — absolute path to the jobspec.json
#
# Behavior:
#   * cd to BUILD_WORKSPACE_DIRECTORY so manifest's workdir + env
#     files resolve against the user's checkout.
#   * Read manifest_path from the jobspec; pass it to `train`.
#   * Forward any further `bazel run -- <extra args>` to the CLI.

set -euo pipefail

if [[ $# -lt 2 ]]; then
    echo "fatal: runner.sh requires CLI_PATH and SPEC_PATH" >&2
    exit 2
fi

CLI_PATH="$1"
SPEC_PATH="$2"
shift 2

# `bazel run` sets BUILD_WORKSPACE_DIRECTORY to the workspace root.
# Outside `bazel run` (rare) fall back to PWD so the script is still
# debuggable.
cd "${BUILD_WORKSPACE_DIRECTORY:-$(pwd)}"

# Extract the manifest path from the spec JSON. Python is universally
# available on the dev hosts we target (Mac + Linux) and is the
# lowest-friction JSON reader; jq isn't always installed.
MANIFEST="$(
    python3 -c "import json, sys; print(json.load(open(sys.argv[1]))['manifest_path'])" \
        "$SPEC_PATH"
)"

echo "runpod_job.run: $CLI_PATH train --manifest $MANIFEST $*" >&2
exec "$CLI_PATH" train --manifest "$MANIFEST" "$@"
