# Architecture

The design intent. Read this before you change the IR builder.

## One parse, two IRs

```text
                 ┌──────────────────────┐
                 │  pulldown-cmark      │
   source ──▶───▶│  event stream        │───▶─── shared walk
                 └──────────────────────┘
                          │
              ┌───────────┴────────────┐
              ▼                        ▼
        ┌──────────┐             ┌──────────┐
        │ flat IR  │             │ tree IR  │
        │  (lint)  │             │  (fmt)   │
        └──────────┘             └──────────┘
              │                        │
              ▼                        ▼
        Vec<Diagnostic>          formatted output
```

Both IRs are built from the same event walk so we parse once. The split keeps the linter cheap (no allocation per node,
no nested visitors) and the formatter expressive (each construct owns a `pretty()` method that emits a Wadler/Lindig
`Doc`).

The `Document` type wraps both IRs plus the source, line index, math regions, and suppression map. Linters see a
`&Document` and a `&mut Vec<Diagnostic>`; the formatter walks the tree IR top-down.

## Math regions overlay

The math scanner (`src/format/math.rs`) runs **before** the event walk and produces a sorted list of byte ranges. The IR
builder consults this list when descending into events and emits a `NodeKind::Math` leaf with the verbatim source slice
in place of the events that would otherwise be generated inside the region.

This is the design choice that makes mdwright math-resilient. See [Math regions](../concepts/math-regions.md) for the
user-facing view.

## Layout algebra

The formatter does not emit strings directly. Each `pretty()` method returns a `Doc` from a Wadler/Lindig algebra:
`text`, `nest`, `line`, `group`, `concat`, `nil`. A rendering pass takes the `Doc` and a target column width and
produces text.

Why an algebra: it makes wrap behaviour declarative. A group either fits on one line or breaks onto multiple; the
rendering pass decides, not the construct. Adding a new construct means implementing one `pretty()` method; the
rendering pass stays the same.

The implementation is in `src/format/doc.rs`; the algebra is small (~200 lines) and standard.

## Escape policy

Markdown's escape rules are context-dependent: `*` is special at the start of a paragraph, neutral inside a code span,
special again inside an emph. The formatter encodes this with an `EscapeContext` value threaded through `pretty()`. Each
construct that opens a new context (link text, table cell, footnote body) pushes a new context; constructs that emit
text consult the top of the stack.

Wrong escape policy is the most common source of round-trip failures. The [`gfm_spec_snapshot`](../deviations.md) test
catches them; the fix is almost always in the escape policy, not in the text emission.

## Doc tests

The `tests/docs_examples.rs` suite walks `docs/src/**/*.md` and validates every fenced code block:

- ```` ```markdown ```` / ```` ```md ```` → must parse with `pulldown-cmark` (no panic; non-empty event stream for
  non-empty input).
- ```` ```toml ```` → must parse with `Config::load_explicit`.
- ```` ```toml,no-check ```` → skipped. Use this fence for non-config TOML (e.g. `book.toml`, `pyproject.toml` excerpts
  that show structure but are not valid config payloads).

A PR that introduces a broken example fails CI. The convention is invisible to mdBook (which treats the language tag as
a CSS class) but the test sees it.

## Where to look

| Want to change…         | Edit…                                                   |
| ----------------------- | ------------------------------------------------------- |
| A lint rule             | `src/stdlib/<rule>.rs` + `src/stdlib/explain/<rule>.md` |
| How a construct formats | `src/format/<construct>.rs`                             |
| Math recognition        | `src/format/math.rs`                                    |
| Escape policy           | `src/format/escape.rs`                                  |
| Wrap algorithm          | `src/format/doc.rs`                                     |
| Event-to-IR mapping     | `src/ir.rs`                                             |
| Config schema           | `src/config.rs` + `build.rs` (regen `configuration.md`) |
| CLI surface             | `src/bin/mdwright.rs` (regen via `cargo xtask doc-cli`) |
