# mdwright

A math-resilient Markdown linter and round-trip formatter for technical writing. Two pipelines share one event walk
over `pulldown-cmark`: a flat IR feeds the lint rules; a typed block / inline IR drives the formatter, where each
construct owns its own `pretty()` method. See `README.md` for user-facing behaviour and `CHANGELOG.md` for the current
release surface.

## Read the Nearest Guide

Before working in a directory, read any `AGENTS.md` that applies. Nested guides override this root guide where they are
more specific. Today only this root file exists; add a nested guide where local rules start to drift from the root.

## Current State

mdwright is a single crate (`src/lib.rs` + `src/bin/mdwright.rs`), not a workspace. The typed-IR redesign shipped as
v0.3.0; the GFM-spec runner at `tests/gfm_spec.rs` and the golden suites under `tests/golden_*` are the load-bearing
correctness fences. Treat `tests/gfm-spec/spec.txt` as vendored upstream — do not edit it; record deviations through
the snapshot / allowlist mechanism documented in `docs/deviations.md`.

## Rust Commands

There is no Makefile. Drive the crate through `cargo` directly:

- `cargo check --all-targets` — type-check the crate (fast).
- `cargo nextest run` — run tests. For one suite use `cargo nextest run --test gfm_spec`. Do not use `cargo test`.
- `cargo clippy --all-targets -- -D warnings` — lint at the level the `Cargo.toml` `[lints]` block expects.
- `cargo fmt` — format.
- `cargo bench` — Criterion benches (`lint_bench`, `format_bench`).

Spec-coverage sweep: `cargo test --release --test gfm_spec gfm_spec_coverage -- --nocapture`.

## Discipline

- No backward compatibility. mdwright is pre-1.0 with a single primary consumer; delete old code paths rather than deprecate them.
- No workarounds, hacks, `TODO`s, or placeholders. Build the intended functionality, not a simplified subset that
    compiles.
- `unsafe` is `forbid`den crate-wide (see `Cargo.toml`). Keep it that way.
- Fix bugs at their root. If the cause lives in the IR builder or the recogniser, fix it there rather than patching the
    pretty-printer downstream.
- Frontload breaking IR changes before building features that depend on them.

## Test Discipline

- Add fixtures before code for block-level formatter work (`tests/golden_block/` and its siblings).
- Property tests live in `tests/properties.rs` with generators in `tests/common/`; regression seeds in
    `tests/regressions/` and `tests/properties.proptest-regressions` must not be deleted.
- The GFM-spec runner has a fast subset by default and a full baseline sweep; treat any new failure as either a real
    regression or an explicit, documented allowlist entry — never both silently.
- For fuzz-found bugs, land the minimised repro under `tests/regressions/` in the same change as the fix.
