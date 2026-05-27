# rules_runpod

Bazel-native RunPod orchestration: typed manifest targets, declarative job + pod rules, idiomatic CLI + TUI toolchain

## Status: v0.0.1 — scaffold

No public surface yet. See `CHANGELOG.md` for what has shipped.

## Install

`.bazelrc`:

```
common --registry=https://raw.githubusercontent.com/fastverk/bazel-registry/main/
common --registry=https://bcr.bazel.build/
```

`MODULE.bazel`:

```python
bazel_dep(name = "rules_runpod", version = "0.0.1")
```
