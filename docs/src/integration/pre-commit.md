# Pre-commit

mdwright ships a `.pre-commit-hooks.yaml` at its repo root, so adding it to a project that uses the
[`pre-commit`](https://pre-commit.com) framework is a single `repos:` entry.

## Quickest path: prebuilt binary

If contributors already have `mdwright` on their `$PATH` (e.g. via `cargo binstall mdwright` or a
GitHub release tarball), the `-system` variants avoid any toolchain dance:

```yaml,no-check
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jcreinhold/mdwright
    rev: v0.1.0
    hooks:
      - id: mdwright-check-system
      - id: mdwright-fmt-check-system
```

`mdwright-check-system` runs `mdwright check --check`; `mdwright-fmt-check-system` runs `mdwright
fmt-check`. Both exit non-zero on issues, blocking the commit.

## Letting pre-commit build mdwright

If you don't want to require an out-of-band install, the source-build hooks invoke
`cargo run -p mdwright` from the checked-out repository. First commit after a clean cache takes
~30 s; subsequent runs reuse Cargo's cache.

```yaml,no-check
repos:
  - repo: https://github.com/jcreinhold/mdwright
    rev: v0.1.0
    hooks:
      - id: mdwright-check
      - id: mdwright-fmt-check
```

Each contributor needs a Rust toolchain on the machine running the hook.

## Available hook IDs

| ID                          | Equivalent CLI            | Language  |
| --------------------------- | ------------------------- | --------- |
| `mdwright-check`            | `mdwright check --check`  | `system` via Cargo |
| `mdwright-fmt`              | `mdwright fmt`            | `system` via Cargo |
| `mdwright-fmt-check`        | `mdwright fmt-check`      | `system` via Cargo |
| `mdwright-check-system`     | `mdwright check --check`  | `system`  |
| `mdwright-fmt-system`       | `mdwright fmt`            | `system`  |
| `mdwright-fmt-check-system` | `mdwright fmt-check`      | `system`  |

The `mdwright-fmt` / `mdwright-fmt-system` hooks rewrite files in place; combine with `git add` in a
post-formatting workflow, or prefer `mdwright-fmt-check` in CI gates that should never auto-commit.

## Performance notes

`pre-commit` invokes hooks once per *batch* of matching files, not once per file, so per-invocation
startup cost is paid once per `git commit` (not once per changed file). The binary's cold-start is
well under 50 ms on Linux release builds.

## See also

- [GitHub Actions](github-actions.md) — server-side CI gate.
- [Editor integrations](editor-integrations.md) — fix-on-save flow.
