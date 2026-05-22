# Architecture

A short tour of the mdwright workspace. For the rendered, full treatment see
[Architecture overview](https://jcreinhold.github.io/mdwright/extending/architecture.html) on the docs site; this file
is a map for someone reading the repo directly.

## Crate stack

mdwright is a virtual Cargo workspace under `crates/`. Each crate hides one kind of knowledge; downstream crates depend
only on layers below.

```text
Surfaces      mdwright (CLI)        mdwright-lsp
Engines       mdwright-format       mdwright-lint
Glue          mdwright-config
Document      mdwright-document
Math spans    mdwright-math
TeX bodies    mdwright-latex
```

- `mdwright-latex` — TeX math-body parsing, Unicode terminal layout, source translation.
- `mdwright-math` — Markdown math-region scanning, delimiter policy, balance diagnostics.
- `mdwright-document` — pulldown invocation behind a containment boundary; recognised Markdown facts with stable source
  coordinates. Every downstream crate reads `Document`, not pulldown events.
- `mdwright-config` — TOML schema and discovery walk.
- `mdwright-format` — verified byte-rewrite formatting. Identity by default; canonicalise and wrap families are gated by
  `semantically_equivalent`.
- `mdwright-lint` — rule trait, dispatcher, suppression handling, the `stdlib` rule set.
- `mdwright` — the CLI binary, file discovery, terminal output, exit-code policy.
- `mdwright-lsp` — LSP delivery over stdio (`tower-lsp`), launched by `mdwright lsp`.

## Deeper documents

The deeper architecture notes live in `docs/architecture/` and are surfaced on the docs site under the **Architecture**
section:

- `crate-boundaries.md` — what each crate owns vs. delegates.
- `parser-boundary.md` — why pulldown is contained in `mdwright-document`.
- `formatter-rewrite-boundary.md` — rewrite-family ownership and verification.
- `pulldown-model.md` — pulldown quirks that mdwright depends on, with drift tests.
- `mdformat-parity.md` — classified divergences from mdformat.
- `parser-backend-audit.md`, `test-matrix.md`, `stability.md`, `latex-boundary-and-dependency-audit.md` — assorted
  release-gate evidence.

## See also

- [`CLAUDE.md`](CLAUDE.md) — repo-wide working agreements.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — PR conventions and MSRV-bump policy.
- [`docs/src/reference/public-api.md`](docs/src/reference/public-api.md) — every public Rust item, mapped to the crate
  that owns it.
