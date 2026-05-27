#!/usr/bin/env bash
# runpod_job.run — invoke `runpod-cli train <manifest>` against the
# manifest declared by the `runpod_job` rule. `bazel run
# //path:<job_name>.run` lands here.
#
# Args (injected by the macro via $(rlocationpath ...)):
#   $1 — runfiles-relative path to the runpod-cli binary
#   $2 — runfiles-relative path to the jobspec.json
#   $3 — runfiles-relative path to the manifest TOML
#
# Behavior:
#   * Initialize Bazel's bash runfiles library and use `rlocation` to
#     map each argv path to a real absolute path inside the runfiles
#     tree. Works uniformly for source-tree files and bazel-bin
#     artifacts.
#   * `cd` to BUILD_WORKSPACE_DIRECTORY so the manifest's
#     `workdir = "."` resolves against the user's checkout.
#   * Forward any further `bazel run -- <extra args>` to the CLI.

set -uo pipefail

# --- begin runfiles.bash initialization v3 ---
# https://github.com/bazelbuild/bazel/blob/main/tools/bash/runfiles/runfiles.bash
set +e
f=bazel_tools/tools/bash/runfiles/runfiles.bash
source "${RUNFILES_DIR:-/dev/null}/$f" 2>/dev/null || \
  source "$(grep -sm1 "^$f " "${RUNFILES_MANIFEST_FILE:-/dev/null}" | cut -f2- -d' ')" 2>/dev/null || \
  source "$0.runfiles/$f" 2>/dev/null || \
  source "$(dirname "$0").runfiles/$f" 2>/dev/null || \
  source "$(dirname "$(readlink -f "$0" 2>/dev/null || echo "$0")").runfiles/$f" 2>/dev/null || \
  { echo>&2 "ERROR: cannot find $f (runfiles library missing)"; exit 1; }
f=
set -e
# --- end runfiles.bash initialization v3 ---

if [[ $# -lt 3 ]]; then
    echo "fatal: runner.sh requires CLI_PATH, SPEC_PATH, and MANIFEST_PATH" >&2
    exit 2
fi

CLI_PATH="$(rlocation "$1")"
SPEC_PATH="$(rlocation "$2")"
MANIFEST_PATH="$(rlocation "$3")"
shift 3

if [[ -z "$CLI_PATH"      || ! -f "$CLI_PATH"      ]]; then echo "fatal: cli not found"      >&2; exit 3; fi
if [[ -z "$SPEC_PATH"     || ! -f "$SPEC_PATH"     ]]; then echo "fatal: jobspec not found"  >&2; exit 3; fi
if [[ -z "$MANIFEST_PATH" || ! -f "$MANIFEST_PATH" ]]; then echo "fatal: manifest not found" >&2; exit 3; fi

cd "${BUILD_WORKSPACE_DIRECTORY:-$(pwd)}"

echo "runpod_job.run: $CLI_PATH train --manifest $MANIFEST_PATH $*" >&2
echo "  (jobspec metadata at: $SPEC_PATH)" >&2
exec "$CLI_PATH" train --manifest "$MANIFEST_PATH" "$@"
