# Contributing to mdwright

Thanks for taking the time. This guide covers how to run the test suite locally, the MSRV policy, and what CI requires
before a change can land.

## Running the tests locally

mdwright is a virtual Cargo workspace. Drive it through `cargo` directly:

```bash
cargo check --workspace --all-targets              # fast type-check
cargo nextest run --workspace --no-fail-fast       # full test suite
cargo clippy --workspace --all-targets -- -D warnings  # lints
cargo fmt --check                                  # formatting
```

CI uses `cargo nextest run --workspace` on every push. To reproduce a CI run exactly:

```bash
cargo nextest run --workspace --release --locked
cargo nextest run -p mdwright --test gfm_spec --release --locked
cargo nextest run -p mdwright --test properties --release --locked
```

For the full GFM spec sweep (slower; used for coverage triage rather than gating):

```bash
cargo test --release -p mdwright --test gfm_spec gfm_spec_coverage -- --nocapture
```

## MSRV policy

The minimum supported Rust version is declared in `Cargo.toml` as `rust-version = "1.91"` and exercised by the CI
matrix.

**Bumping the MSRV is a minor-version change.** Bump it only in a `0.minor` (pre-1.0) or `1.minor` (post-1.0) release.
Never bump the MSRV in a patch release—downstream users rely on patches being safe to take.

When bumping, update both:

1. `rust-version` in the workspace `Cargo.toml`.
2. The `rust:` matrix entry in `.github/workflows/ci.yml`.

## Required CI checks before merge

Every change must show green on:

- All `test` matrix jobs: `{ubuntu-latest, macos-latest, windows-latest} × {stable, MSRV}`—six jobs total.
- `clippy` (`cargo clippy --workspace --all-targets --locked -- -D warnings` on Linux/stable).
- `fmt` (`cargo fmt --check` on Linux/stable).

`main` is configured (via GitHub branch protection) to require these checks before merge. If you have permission to
adjust branch protection, keep the required-check list in sync with the workflow's job names; if you don't, the
maintainers will.

## House rules worth knowing

- No backward compatibility shims—mdwright is pre-1.0 with a single primary consumer; delete old paths rather than
  deprecate them.
- `unsafe` is forbidden crate-wide. Keep it that way.
- Add fixtures before code for block-level formatter work (`crates/mdwright/tests/golden_block/` and friends).
- For fuzz-found bugs, land the minimised repro under `crates/mdwright/tests/regressions/` in the same change as the
  fix.

See `AGENTS.md` (root and any nested guides) for the full discipline notes.
