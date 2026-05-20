# mdwright

A math-resilient Markdown linter and round-trip formatter for technical writing.

## Read the Nearest Guide

Before working in a directory, read any `AGENTS.md` that applies. Nested guides override this root guide where they are
more specific. Today only this root file exists; add a nested guide where local rules start to drift from the root.

## Current State

mdwright is a virtual Cargo workspace. The command package lives at `crates/mdwright` and owns the `mdwright` binary.
Library users depend directly on component crates:

- `mdwright-document`: source canonicalisation, pulldown invocation, parser panic containment, document facts, ranges,
  references, frontmatter, lists, code/HTML exclusions, and parse options.
- `mdwright-math`: pure TeX/math span recognition, render conversion, and body normalisation.
- `mdwright-format`: identity structural emit plus transactional, verified byte rewrites for opt-in canonicalisation and
  wrapping.
- `mdwright-lint`: diagnostics, suppression handling, safe fixes, rule execution, and standard rules.
- `mdwright-config`: TOML schema, config discovery, and raw config to parse/format/lint policy.
- `mdwright-lsp`: editor delivery over LSP.

See `docs/architecture/crate-boundaries.md` and `docs/architecture/parser-boundary.md` for the current boundaries.

## Rust Commands

There is no Makefile. Drive the workspace through `cargo` directly:

- `cargo check --workspace --all-targets`: type-check everything.
- `cargo nextest run --workspace --no-fail-fast`: run tests. For one suite use
  `cargo nextest run -p mdwright --test gfm_spec`. Do not use `cargo test` unless a documented coverage command
  specifically requires it.
- `cargo clippy --workspace --all-targets -- -D warnings`: lint at the level the workspace `[lints]` block expects.
- `cargo fmt`: format.
- `cargo bench -p mdwright --bench format_bench --bench lint_bench`: Criterion benches.
- `cargo xtask production-soak --corpus-root <PATH>`: release-oriented corpus soak (`<PATH>` is a directory of Markdown
  files; set via `MDWRIGHT_CORPUS_ROOT` or pass `--corpus-root`).
- `cargo xtask mdformat-parity --corpus-root <PATH> --corpus-name <NAME> --mdwright-config <PATH> --mdformat-config xtask/fixtures/mdformat-parity/mdformat.toml`:
  compare mdwright output against mdformat with a checked classification table for intentional divergences. The mdformat
  config is a parity fixture, not the repository formatter.
- `cargo xtask parser-audit --case-set all --ensure-tools --include-comrak`: compare mdwright's parser backend against
  cmark-gfm expected/rendered HTML, with optional comrak diagnostics.
- `cargo xtask release-evidence --output target/mdwright/release`: aggregate local release-candidate evidence into JSON
  and Markdown reports.

Spec-coverage sweep: `cargo test --release -p mdwright --test gfm_spec gfm_spec_coverage -- --nocapture`.

## Discipline

- No backward compatibility by default. mdwright is pre-1.0 and has one primary consumer; delete false abstractions
  rather than preserving stale paths.
- No workarounds, hacks, `TODO`s, or placeholders. Build the intended functionality, not a simplified subset that
  compiles.
- `unsafe` is forbidden crate-wide. Keep it that way.
- Fix bugs at their owning boundary. Parser panics are contained in `mdwright-document`; formatter rewrite mistakes are
  rejected by `mdwright-format` verification; lint bugs belong in `mdwright-lint`.
- Do not add a crate boundary, public facade, trait, or option unless it hides a real volatile decision behind a small
  interface.

## Test Discipline

- Add fixtures before code for formatter behavior (`crates/mdwright/tests/golden_*` and regressions under
  `crates/mdwright/tests/regressions/`).
- Property tests live in `crates/mdwright/tests/properties.rs` with generators in `crates/mdwright/tests/common/`;
  regression seeds in `crates/mdwright/tests/properties.proptest-regressions` must not be deleted.
- Treat `crates/mdwright/tests/gfm-spec/spec.txt` as vendored upstream. Do not edit it; record deviations through the
  snapshot / allowlist mechanism documented in `docs/deviations.md`.
- For fuzz-found bugs, land the minimised repro under `crates/mdwright/tests/regressions/` in the same change as the
  fix.
- `fuzz/artifacts/**` must be empty before commit. Diagnose, minimise, promote to a regression, or delete stale
  non-reproducing artifacts.
