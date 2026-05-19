# Architecture

The design intent. Read this before you change document recognition, linting, or formatting.

## Workspace boundaries

Each crate hides a different kind of knowledge. Read the layers top-down as a dependency stack —
each depends only on layers below it:

```text
Surfaces      mdwright (CLI)        mdwright-lsp
Engines       mdwright-format       mdwright-lint
Glue          mdwright-config
Document      mdwright-document
Math          mdwright-math
```

- `mdwright-math` — pure TeX / math scanning and normalisation.
- `mdwright-document` — source coordinates, pulldown invocation, parse options, recognised
  Markdown facts.
- `mdwright-config` — interprets user config files into document, format, and lint policy.
- `mdwright-format` — formatting options and the transactional rewrite engine.
- `mdwright-lint` — diagnostics, rule execution, suppression, safe fixes.
- `mdwright` — the command-line binary: file discovery, terminal output, process exit policy.
- `mdwright-lsp` — editor-state delivery over LSP.

The repository root is a virtual workspace. There is no facade crate; library users depend
directly on the crate that owns the capability they need.

## Document facts

`Document` is parse/query only. It wraps the original source, canonical source mapping, line index, pulldown-derived
events, references, lists, code and HTML exclusion ranges, heading attributes, frontmatter, and math regions. Lint rules
and formatter rewrite producers consume these immutable facts instead of invoking pulldown independently.

Recognition policy lives in `ParseOptions`. Formatting policy lives in `FmtOptions`.

## Math regions

The math scanner lives in `mdwright-math` and knows only about strings and byte ranges. The document crate supplies
Markdown exclusion ranges, stores accepted math regions, and gives downstream crates one stable inventory to query.

This is the design choice that makes mdwright math-resilient. See [Math regions](../concepts/math-regions.md) for the
user-facing view.

## Formatting

Default formatting is identity emit: source bytes survive unchanged except for document-boundary policies. Opt-in style
canonicalisation and wrapping are represented as rewrite candidates owned by the current document snapshot.

Only the private rewrite engine in `mdwright-format` may apply formatter byte edits. It orders candidates, rejects
overlaps, applies a batch to a scratch buffer, verifies Markdown and math signatures, falls back to single-candidate
isolation when needed, and iterates to a fixed point.

## Linting

`RuleSet` owns rule execution. Callers parse a `Document`, then call `rules.check(&doc)` or `rules.check_with(&doc,
opts)`. Suppressions, diagnostic sorting, standard-rule registry construction, and safe-fix application are lint-crate
details.

## Doc tests

The `crates/mdwright/tests/docs_examples.rs` suite walks `docs/src/**/*.md` and validates every fenced code block:

- ```` ```markdown ```` / ```` ```md ```` → must parse with `pulldown-cmark` (no panic; non-empty event stream for
  non-empty input).
- ```` ```toml ```` → must parse with `Config::load_explicit`.
- ```` ```toml,no-check ```` → skipped. Use this fence for non-config TOML (e.g. `book.toml`, `pyproject.toml` excerpts
  that show structure but are not valid config payloads).

A PR that introduces a broken example fails CI. The convention is invisible to mdBook (which treats the language tag as
a CSS class) but the test sees it.

## Where to look

| Want to change…      | Edit…                                                              |
| -------------------- | ------------------------------------------------------------------ |
| A lint rule          | `crates/mdwright-lint/src/stdlib/<rule>.rs` + its explanation      |
| Document recognition | `crates/mdwright-document/src/`                                    |
| Math recognition     | `crates/mdwright-math/src/`                                        |
| Formatter rewrites   | `crates/mdwright-format/src/format/`                               |
| Wrap algorithm       | `crates/mdwright-format/src/format/wrap_pass.rs`                   |
| Config schema        | `crates/mdwright-config/src/config.rs` + `xtask/src/config_docs.rs` |
| CLI surface          | `crates/mdwright/src/cli.rs`                                       |
| LSP surface          | `crates/mdwright-lsp/src/lsp.rs`                                   |
