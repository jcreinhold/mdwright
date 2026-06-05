#!/usr/bin/env bash
# Ensure the locally installed cargo-public-api matches the version CI pins.
# cargo-public-api's output format changes between releases (e.g. 0.52 drops
# the parameter names 0.51 prints), so an unpinned local tool reports spurious
# drift against the committed docs/api-review baselines. The version is read
# from the CI workflow so the pin stays a single source of truth.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

pinned="$(awk -F': *' '/CARGO_PUBLIC_API_VERSION:/{print $2; exit}' .github/workflows/ci.yml)"
if [ -z "$pinned" ]; then
  echo "::error::could not read CARGO_PUBLIC_API_VERSION from .github/workflows/ci.yml" >&2
  exit 1
fi

installed="$(cargo public-api --version 2>/dev/null | awk '{print $2}' || true)"
if [ "$installed" = "$pinned" ]; then
  exit 0
fi

echo "cargo-public-api $pinned required (found: ${installed:-none}); installing to match CI"
cargo install cargo-public-api --version "$pinned" --locked
