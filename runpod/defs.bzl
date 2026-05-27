"""rules_runpod public API.

```starlark
load("@rules_runpod//runpod:defs.bzl",
     "runpod_manifest", "runpod_job", "runpod_pod")
```

Three macros (currently thin pass-throughs to the underlying
rules; future versions add `bazel run` wrappers that invoke the
CLI with the rendered spec).
"""

load(
    "//runpod/private:rules.bzl",
    _runpod_job = "runpod_job",
    _runpod_manifest = "runpod_manifest",
    _runpod_pod = "runpod_pod",
)

runpod_manifest = _runpod_manifest
runpod_job = _runpod_job
runpod_pod = _runpod_pod
