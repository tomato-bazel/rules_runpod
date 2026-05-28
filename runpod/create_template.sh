#!/usr/bin/env bash
# runpod_template.create — register a template from its spec.
#
# `bazel run //path:<template_name>.create` lands here.
#
# Args (positional, injected by the macro via $(rlocationpath …)):
#   $1 — runpod-cli binary
#   $2 — TemplateCreateInput JSON spec
#
# Prints the created template JSON (including the assigned `id`)
# to stdout. Capture the id to reference the template from a
# `runpod endpoint create --template-id <id>` or another tool.

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
    echo "fatal: create_template.sh requires CLI_PATH and TEMPLATE_SPEC" >&2
    exit 2
fi

CLI_PATH="$(rlocation "$1")"
TEMPLATE_SPEC="$(rlocation "$2")"

if [[ -z "$CLI_PATH"      || ! -x "$CLI_PATH"      ]]; then echo "fatal: cli not found at $1"     >&2; exit 3; fi
if [[ -z "$TEMPLATE_SPEC" || ! -f "$TEMPLATE_SPEC" ]]; then echo "fatal: template spec not found" >&2; exit 3; fi

cd "${BUILD_WORKSPACE_DIRECTORY:-$(pwd)}"

echo "===> creating template from $TEMPLATE_SPEC" >&2
exec "$CLI_PATH" template create-from-spec "$TEMPLATE_SPEC"
