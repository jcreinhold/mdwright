#!/usr/bin/env bash
set -euo pipefail

crates=(
  mdwright-latex
  mdwright-math
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
