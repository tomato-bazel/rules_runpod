#!/usr/bin/env bash
# runpod_network_volume.create — create (or reuse) a network volume.
#
# `bazel run //path:<volume_name>.create` lands here. Idempotent:
# the CLI's `volume create-from-spec` reuses an existing same-name
# volume in the same data center instead of duplicating.
#
# Args (positional, injected by the macro via $(rlocationpath …)):
#   $1 — runpod-cli binary
#   $2 — NetworkVolumeCreateInput JSON spec
#
# Prints the volume id to stdout (the value endpoints/pods resolve
# by name at deploy time).

set -uo pipefail

# --- begin runfiles.bash initialization v3 ---
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

if [[ $# -lt 2 ]]; then
    echo "fatal: create_volume.sh requires CLI_PATH and VOLUME_SPEC" >&2
    exit 2
fi

CLI_PATH="$(rlocation "$1")"
VOLUME_SPEC="$(rlocation "$2")"

if [[ -z "$CLI_PATH"    || ! -x "$CLI_PATH"    ]]; then echo "fatal: cli not found at $1"   >&2; exit 3; fi
if [[ -z "$VOLUME_SPEC" || ! -f "$VOLUME_SPEC" ]]; then echo "fatal: volume spec not found" >&2; exit 3; fi

cd "${BUILD_WORKSPACE_DIRECTORY:-$(pwd)}"

echo "===> creating (or reusing) volume from $VOLUME_SPEC" >&2
exec "$CLI_PATH" volume create-from-spec "$VOLUME_SPEC"
