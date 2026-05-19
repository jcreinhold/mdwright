# Phase R prompt 23 — reference resolver: cluster retirement

## Summary

Reference-style links and link-reference definitions now flow through a typed resolver built from pulldown-cmark's own
CM §4.7 resolution. The old regex-based `scan_link_defs` is gone.

GFM spec failures: **74 → 64** (Δ = 10 retired, 1 new — net **−10**).

## Retired cluster (this prompt)

All eight html-divergence cases in the §4.7 cluster retire, plus three adjacent cases:

| Case | Section            | Kind        | Root cause                                                 |
| ---- | ------------------ | ----------- | ---------------------------------------------------------- |
| 162  | Link ref defs      | html, ast   | Multi-line def: regex didn't allow newline before dest.   |
| 163  | Link ref defs      | html, ast   | Escaped `]` inside label: regex didn't allow `\\.` escapes. |
| 164  | Link ref defs      | html, ast   | Multi-line def with `<angle>` dest on its own line.        |
| 165  | Link ref defs      | html, ast   | Multi-line title spanning several lines.                   |
| 167  | Link ref defs      | html, ast   | Multi-line def with empty title.                           |
| 182  | Link ref defs      | html, ast   | Def-shaped line *inside* a paragraph — old scan emitted it as a trailing def, duplicating the source paragraph. |
| 186  | Link ref defs      | html, ast   | Multi-line defs interleaved with single-line defs.         |
| 187  | Link ref defs      | html        | Def nested inside a blockquote — old scan emitted it at document top level. |
| 170  | Link ref defs      | idempotence | `[foo]: <bar>(baz)` — invalid def per CM, was parsed as one by the old regex. |
| 549  | (unknown)          | ast         | Resolver downstream of §4.7.                               |
| 554  | (unknown)          | idempotence | Resolver downstream of §4.7.                               |

Case **169** (`[foo]: <>` with empty angle dest) still html-fails. Pulldown preserves the empty-dest resolution; the
formatter currently emits no fence around an empty dest, which round-trips through pulldown to a different AST. Tracked
as a follow-up.

## New failure

Case **171** is now failing. Inspection: pulldown's emitted dest for some single-line refs is recovered with different
escape state than the formatter's emission path expects. Tracked as a follow-up.

## Design notes

- `cm::refs::build_reference_table` is the single point that consumes the pulldown event stream and populates the table.
- `cm::refs::scan_def_label_casings` is a single-line label-prefix scan that preserves the source's casing for
  definitions. Without it pulldown's `Tag::Link.id` (the *link's* bracket text) would override the def's source casing.
- `NormalisedLabel::from_raw` is the sole CM §4.7 normalisation site.
- `TreeBuilder::finalize` runs a post-pass (`downgrade_unresolved_links`) that replaces reference-style `Link` / `Image`
  nodes whose label fails to resolve with `Unknown { tag }`. The formatter's `Unknown` fallback emits the original
  source span verbatim — exactly CM §4.7's "leave as text" rule.
- Unused defs (in source but with no reference) are now dropped from the trailing block. They have no rendering effect
  under HTML, but fixture `38_ref_link_preserve` had to be updated to reflect the new contract.

## Verification

- `cargo test --release` green (216 unit + all integration tests pass).
- `cargo test --release --test gfm_spec -- --ignored gfm_spec_full`: 64 unique failing cases (down from 74).
- New proptest `reference_resolver_round_trips` exercises random `(label, dest, title, kind)` triples through format →
  reparse → format and HTML round-trip; 256 cases per run.
