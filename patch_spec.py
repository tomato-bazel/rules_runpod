"""Patch the upstream RunPod OpenAPI document so it conforms to what
progenitor + typify expect.

RunPod's spec has three classes of bugs we work around in-flight,
keeping the vendored copy a pristine snapshot:

1. **Duplicate operationIds.** Both `PATCH /pods/{podId}` and
   `POST /pods/{podId}/update` declare `operationId: UpdatePod`,
   tripping progenitor with "duplicate operation ID". We append
   `ViaPost` to the legacy POST variants so PATCH stays canonical.

2. **`type: object` with `items`.** Several properties (Pod.env,
   PodCreateInput.env, Template.env, …) are declared as object types
   carrying an `items` keyword, which only belongs to arrays. The
   intent is a map of string→T, which OpenAPI expresses as
   `additionalProperties: <T>`. We migrate `items` to
   `additionalProperties` and drop the broken keyword.

3. **Wrong-type `example` values.** `Pod.costPerHr` declares
   `type: number` but supplies the example as the string "0.74".
   typify's value validation rejects this. We drop `example` fields
   that don't match their declared schema type rather than try to
   coerce them — they're documentation, not data.

4. **`format` hints that pull in chrono / uuid / Ipv4Addr.**
   progenitor maps `format: date-time` to `chrono::DateTime<Utc>`,
   `format: uuid` to `uuid::Uuid`, and `format: ipv4` to
   `std::net::Ipv4Addr`. The first two suffer a trait-identity
   mismatch (rules_openapi 0.2 doesn't thread chrono/uuid through
   its attrs). The third is worse: RunPod's API sends an empty
   string `""` for `Pod.publicIp` before a pod has been assigned
   one, and `Ipv4Addr`'s serde deserializer rejects empty strings,
   crashing the whole response parse.

   The safe play is to strip these formats so progenitor emits
   plain `String`. Lose some type safety, gain a working build +
   tolerate RunPod's "" sentinel.

   We leave non-typified formats (e.g. the bogus `currency`) alone —
   typify ignores anything it doesn't recognize.

Runs as a Bazel genrule action; see tools/runpod-cli/BUILD.bazel.
"""

from __future__ import annotations

import json
import sys
from typing import Any


def _dedup_operation_ids(spec: dict) -> list[tuple[str, str, str]]:
    renamed: list[tuple[str, str, str]] = []
    for path, methods in spec.get("paths", {}).items():
        if not path.endswith("/update"):
            continue
        for method, op in methods.items():
            if not isinstance(op, dict) or "operationId" not in op:
                continue
            if method.lower() != "post":
                continue
            old = op["operationId"]
            new = f"{old}ViaPost"
            op["operationId"] = new
            renamed.append((path, old, new))
    return renamed


def _fix_object_items(obj: Any, path: str, fixes: list[str]) -> None:
    """Rewrite `type: object` schemas that carry `items` into
    `additionalProperties`-shaped maps."""
    if isinstance(obj, dict):
        if obj.get("type") == "object" and "items" in obj:
            obj["additionalProperties"] = obj.pop("items")
            fixes.append(path)
        for k, v in obj.items():
            _fix_object_items(v, f"{path}.{k}", fixes)
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            _fix_object_items(v, f"{path}[{i}]", fixes)


def _value_matches_type(v: Any, t: str) -> bool:
    if v is None:
        # Spec lacks `nullable: true` on these fields; typify rejects
        # null on a non-nullable. Drop and let the field be Required.
        return False
    if t == "integer":
        return isinstance(v, int) and not isinstance(v, bool)
    if t == "number":
        return isinstance(v, (int, float)) and not isinstance(v, bool)
    if t == "string":
        return isinstance(v, str)
    if t == "boolean":
        return isinstance(v, bool)
    if t == "array":
        return isinstance(v, list)
    if t == "object":
        return isinstance(v, dict)
    return True  # Unknown type — leave it alone.


def _drop_mistyped_constants(obj: Any, path: str, drops: list[str]) -> None:
    """Remove `example` / `default` values whose type doesn't match the
    declared schema type. typify rejects mismatches; these are
    documentation artefacts in the upstream spec, so we drop rather
    than coerce."""
    if isinstance(obj, dict):
        if "type" in obj:
            t = obj["type"]
            for key in ("example", "default"):
                if key in obj and not _value_matches_type(obj[key], t):
                    drops.append(f"{path}.{key} (type={t}, value={obj[key]!r})")
                    obj.pop(key)
        for k, v in obj.items():
            _drop_mistyped_constants(v, f"{path}.{k}", drops)
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            _drop_mistyped_constants(v, f"{path}[{i}]", drops)


def _add_created_responses(spec: dict, adds: list[str]) -> None:
    """RunPod's `POST /networkvolumes` returns `201 Created` with the
    new volume in the body, but the spec only documents `200`. progenitor
    treats the undocumented 201 as an `UnexpectedResponse` error (the
    volume *is* created, but the client errors before parsing it). Mirror
    the 200 response under 201 so the generated client accepts it and
    deserializes the NetworkVolume."""
    for path, methods in spec.get("paths", {}).items():
        post = methods.get("post") if isinstance(methods, dict) else None
        if not isinstance(post, dict):
            continue
        responses = post.get("responses", {})
        if "201" not in responses and "200" in responses:
            # Only the create endpoints actually return 201; scope by
            # presence of a 200 JSON body on a POST that creates a resource.
            if path.rstrip("/").endswith("networkvolumes"):
                responses["201"] = json.loads(json.dumps(responses["200"]))
                adds.append(f"{path} POST: added 201 (mirrors 200)")


_STRIP_FORMATS = {"date-time", "uuid", "ipv4"}


def _strip_typified_formats(obj: Any, path: str, drops: list[str]) -> None:
    """Remove `format` annotations that progenitor maps to chrono /
    uuid types; see module docstring."""
    if isinstance(obj, dict):
        if obj.get("type") == "string" and obj.get("format") in _STRIP_FORMATS:
            drops.append(f"{path} (format={obj['format']})")
            obj.pop("format")
        for k, v in obj.items():
            _strip_typified_formats(v, f"{path}.{k}", drops)
    elif isinstance(obj, list):
        for i, v in enumerate(obj):
            _strip_typified_formats(v, f"{path}[{i}]", drops)


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: patch_spec.py <input.json> <output.json>", file=sys.stderr)
        return 2
    src, dst = sys.argv[1], sys.argv[2]
    with open(src) as f:
        spec = json.load(f)

    renamed = _dedup_operation_ids(spec)
    object_items_fixes: list[str] = []
    _fix_object_items(spec, "$", object_items_fixes)
    example_drops: list[str] = []
    _drop_mistyped_constants(spec, "$", example_drops)
    format_strips: list[str] = []
    _strip_typified_formats(spec, "$", format_strips)
    created_adds: list[str] = []
    _add_created_responses(spec, created_adds)

    with open(dst, "w") as f:
        json.dump(spec, f, indent=2)

    for path, old, new in renamed:
        print(f"renamed {old} -> {new} ({path})", file=sys.stderr)
    for p in object_items_fixes:
        print(f"object+items -> additionalProperties at {p}", file=sys.stderr)
    for p in example_drops:
        print(f"dropped mistyped value at {p}", file=sys.stderr)
    for p in format_strips:
        print(f"stripped typified format at {p}", file=sys.stderr)
    for p in created_adds:
        print(f"added created-response: {p}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
