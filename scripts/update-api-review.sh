#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

scripts/ensure-cargo-public-api.sh

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

mkdir -p docs/api-review

for crate in "${crates[@]}"; do
  cargo public-api --simplified -p "$crate" >"docs/api-review/${crate}-public.txt"
done
