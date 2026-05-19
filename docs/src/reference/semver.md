# Semver policy

mdwright follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). This page enumerates the **public API
surface** that the version number commits to.

## Covered

A change to any of the following is a breaking change and requires a major-version bump (or a minor bump while we are
pre-1.0—see [Pre-1.0 caveats](#pre-10-caveats) below):

- Every `pub` item exported from `mdwright` (`src/lib.rs` and the modules it re-exports). The bar is "could a downstream
  crate observe the change at compile time."
- CLI subcommands, their flags, and their exit codes. The exit-code mapping appears in
  [`reference/cli.md`](cli.md).
- The configuration schema for `mdwright.toml`, `.mdwright.toml`, and `pyproject.toml [tool.mdwright]`. The schema is
  generated into [`configuration.md`](../configuration.md) by `build.rs` from a single source of truth.
- The `--format=json` (v2) diagnostic schema at
  [`reference/diagnostic-schema.md`](diagnostic-schema.md) and the JSON Schema at
  [`docs/diagnostic-schema.json`](https://github.com/jcreinhold/mdwright/blob/main/docs/diagnostic-schema.json). New
  optional fields are non-breaking; renaming or removing a field is breaking.
- The `LintRule` trait signature in `src/lint.rs`. Adding a method with a default body is non-breaking; adding a method
  without a default, or changing an existing signature, is breaking.

## Not covered

The following are free to change in any release, including patch releases:

- Internal items (anything `pub(crate)` or private). Refactors that move modules around are not breaking unless they
  change a `pub` export.
- The on-disk layout of build artifacts (`target/`), cached state, and intermediate files.
- The prose output of `mdwright explain <rule>`. The rule names and their existence are covered; the wording is not.
- Performance characteristics. We aim not to regress and track this through Criterion benches, but we do not commit to
  a wall-clock floor.
- The contents of `docs/`, `CHANGELOG.md`, and other repo metadata.
- The format of `tracing` output and log lines.

## Pre-1.0 caveats

Until v1.0, minor versions may include breaking API changes. The 0.x sequence is deliberately permissive so the surface
can settle without dragging compatibility shims forward. The discipline still applies—every break appears in
[`CHANGELOG.md`](https://github.com/jcreinhold/mdwright/blob/main/CHANGELOG.md) under **Breaking changes** in the
relevant version's section, with a migration note where the rewrite is non-obvious.

Patch releases (`0.x.Y`) never introduce breaking changes.

## MSRV (minimum supported Rust version)

The MSRV is `rust-version = "1.91"`, declared in `Cargo.toml`. Bumping the MSRV is treated as a minor-version bump
pre-1.0; post-1.0 it will be a major bump. CI runs the test suite on both stable and the MSRV floor on every push.
