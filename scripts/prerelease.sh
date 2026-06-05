#!/usr/bin/env bash
# Local pre-release gate. Mirrors the `verify` job in
# .github/workflows/release.yml command-for-command, so passing here is the fast
# feedback loop for "will the release workflow's gate be green?". Run from a clean
# tree before tagging. Stops on the first failure.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

crates=(
  mdwright-latex
  mdwright-math
  mdwright-mathrender
  mdwright-document
  mdwright-format
  mdwright-lint
  mdwright-config
  mdwright-lsp
  mdwright
)

step() { printf '\n=== %s ===\n' "$1"; }

step "actionlint workflows"
actionlint .github/workflows/*.yml

step "cargo fmt --check"
cargo fmt --check

step "cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets --locked -- -D warnings

step "cargo nextest run --workspace --release"
cargo nextest run --workspace --release --locked

step "cargo doc --workspace --no-deps"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

step "Verify generated docs"
cargo xtask doc-rules --check
cargo xtask doc-cli --check
cargo xtask doc-config --check

step "Build mdBook"
mdbook build docs/

step "Public API diff vs committed baselines"
for crate in "${crates[@]}"; do
  baseline="docs/api-review/${crate}-public.txt"
  if [ ! -f "$baseline" ]; then
    echo "::error::Missing public API baseline: $baseline" >&2
    exit 1
  fi
  actual="$(mktemp)"
  cargo public-api --simplified -p "$crate" >"$actual"
  if ! diff -u "$baseline" "$actual"; then
    echo "::error::Public API drift for $crate. Regenerate with scripts/update-api-review.sh." >&2
    exit 1
  fi
done

step "Package tarball docs.rs simulation"
python3 scripts/check_package_docsrs.py

printf '\nAll pre-release gates passed.\n'
