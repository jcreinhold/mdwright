# downstream—mdwright integration smoke fixture

This is the minimal Markdown project we use to test that mdwright's `pre-commit-hooks.yaml` and `action.yml` remain
working from a downstream consumer's point of view.

- `docs/good.md`—clean Markdown; every mdwright check must pass.
- `docs/bad.md`—contains a known set of defects; `mdwright check --check` must fail and surface specific rule names.
- `.pre-commit-config.yaml`—wires the `-system` hook variants against an `mdwright` already on `$PATH`. The integration
  test (`tests/downstream_integration.rs`) bypasses the `pre-commit` framework entirely and invokes the binary directly,
  so the test stays hermetic; the file documents the consumer-facing usage pattern.

The fixture is not published to crates.io (the package's `include` allowlist excludes `examples/`).
