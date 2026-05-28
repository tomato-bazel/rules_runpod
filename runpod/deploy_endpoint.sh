#!/usr/bin/env bash
# runpod_serverless_endpoint.deploy — provision a template + a
# serverless endpoint, in that order, in a single command.
#
# Behavior:
#   1. POST the template spec → capture the new template id.
#   2. Patch the endpoint spec, replacing the `"templateId":
#      "<resolved-at-deploy-time>"` placeholder with the real id.
#   3. POST the patched endpoint spec → print the resulting
#      endpoint JSON to stdout.
#
# Args (positional, injected by the macro via $(rlocationpath …)):
#   $1 — runpod-cli binary
#   $2 — template spec JSON (TemplateCreateInput shape)
#   $3 — endpoint  spec JSON (EndpointCreateInput shape with
#         templateId placeholder)
#
# Any extra `bazel run -- <args>` are NOT forwarded — both
# subcommands take their config from the JSON specs.
#
# Idempotence: v0 always creates fresh template + endpoint.
# Re-running produces a second pair. Future versions may look
# the template up by name and reuse it.

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

if [[ $# -lt 3 ]]; then
    echo "fatal: deploy_endpoint.sh requires CLI_PATH, TEMPLATE_SPEC, ENDPOINT_SPEC" >&2
    exit 2
fi

CLI_PATH="$(rlocation "$1")"
TEMPLATE_SPEC="$(rlocation "$2")"
ENDPOINT_SPEC="$(rlocation "$3")"

if [[ -z "$CLI_PATH"      || ! -x "$CLI_PATH"      ]]; then echo "fatal: cli not found at $1"      >&2; exit 3; fi
if [[ -z "$TEMPLATE_SPEC" || ! -f "$TEMPLATE_SPEC" ]]; then echo "fatal: template spec not found"  >&2; exit 3; fi
if [[ -z "$ENDPOINT_SPEC" || ! -f "$ENDPOINT_SPEC" ]]; then echo "fatal: endpoint spec not found"  >&2; exit 3; fi

if ! command -v python3 >/dev/null 2>&1; then
    echo "fatal: python3 not on PATH (needed to parse the template-create response)" >&2
    exit 3
fi

# Operate from the user's workspace so any --files-like args
# resolve against the source checkout, not the runfiles tree.
cd "${BUILD_WORKSPACE_DIRECTORY:-$(pwd)}"

echo "===> creating template from $TEMPLATE_SPEC" >&2
TEMPLATE_JSON="$("$CLI_PATH" template create-from-spec "$TEMPLATE_SPEC")"
TEMPLATE_ID="$(echo "$TEMPLATE_JSON" | python3 -c "
import json, sys
try:
    obj = json.loads(sys.stdin.read())
except Exception as e:
    print(f'fatal: template create response not JSON: {e}', file=sys.stderr)
    sys.exit(1)
tid = obj.get('id')
if not tid:
    print(f'fatal: template create response missing id: {obj}', file=sys.stderr)
    sys.exit(1)
print(tid)
")"
echo "  → template id: $TEMPLATE_ID" >&2

PATCHED_SPEC="$(mktemp -t runpod-endpoint-spec.XXXXXX.json)"
trap 'rm -f "$PATCHED_SPEC"' EXIT
python3 - "$ENDPOINT_SPEC" "$TEMPLATE_ID" >"$PATCHED_SPEC" <<'PYEOF'
import json, sys
spec_path, template_id = sys.argv[1], sys.argv[2]
with open(spec_path) as f:
    spec = json.load(f)
if spec.get("templateId") not in (None, "", "<resolved-at-deploy-time>"):
    print(
        f"warning: endpoint spec already has templateId={spec['templateId']!r}; "
        f"overwriting with freshly-created {template_id}",
        file=sys.stderr,
    )
spec["templateId"] = template_id
json.dump(spec, sys.stdout, indent=2)
PYEOF

echo "===> creating endpoint from patched spec" >&2
"$CLI_PATH" endpoint create-from-spec "$PATCHED_SPEC"
