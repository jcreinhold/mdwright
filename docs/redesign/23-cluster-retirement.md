# Reference resolver: cluster retirement

Reference-style links and link-reference definitions flow through a typed resolver built from pulldown-cmark's own CM
§4.7 resolution. The old regex-based `scan_link_defs` is gone.

GFM spec failures went from 74 to 64: 10 retired, 1 new, net −10.

## Retired cases

All eight html-divergence cases in the §4.7 cluster, plus three adjacent ones:

| Case | Section       | Kind        | Root cause                                                                          |
| ---- | ------------- | ----------- | ----------------------------------------------------------------------------------- |
| 162  | Link ref defs | html, ast   | Multi-line def: regex didn't allow newline before dest.                             |
| 163  | Link ref defs | html, ast   | Escaped `]` inside label: regex didn't allow `\\.` escapes.                         |
| 164  | Link ref defs | html, ast   | Multi-line def with `<angle>` dest on its own line.                                  |
| 165  | Link ref defs | html, ast   | Multi-line title spanning several lines.                                            |
| 167  | Link ref defs | html, ast   | Multi-line def with empty title.                                                    |
| 182  | Link ref defs | html, ast   | Def-shaped line inside a paragraph; old scan emitted it as a trailing def, duplicating the source paragraph. |
| 186  | Link ref defs | html, ast   | Multi-line defs interleaved with single-line defs.                                  |
| 187  | Link ref defs | html        | Def nested inside a blockquote; old scan emitted it at document top level.          |
| 170  | Link ref defs | idempotence | `[foo]: <bar>(baz)`: invalid per CM, parsed as one def by the old regex.            |
| 549  | (downstream)  | ast         | Resolver downstream of §4.7.                                                        |
| 554  | (downstream)  | idempotence | Resolver downstream of §4.7.                                                        |

## Still failing

- **169** (`[foo]: <>`, empty angle dest). Pulldown preserves the empty-dest resolution; the formatter emits no fence
  around an empty dest, which round-trips through pulldown to a different AST.
- **171**. Pulldown's emitted dest for some single-line refs is recovered with different escape state than the
  formatter's emission path expects.

## Design notes

- `cm::refs::build_reference_table` is the single point that consumes the pulldown event stream and populates the table.
- `cm::refs::scan_def_label_casings` is a single-line label-prefix scan that preserves the source's casing for
  definitions. Without it, pulldown's `Tag::Link.id` (the link's bracket text) would override the def's source casing.
- `NormalisedLabel::from_raw` is the sole CM §4.7 normalisation site.
- `TreeBuilder::finalize` runs a post-pass (`downgrade_unresolved_links`) that replaces reference-style `Link`/`Image`
  nodes whose label fails to resolve with `Unknown { tag }`. The formatter's `Unknown` fallback emits the original
  source span verbatim—CM §4.7's "leave as text" rule.
- Unused defs (in source but unreferenced) drop from the trailing block. They have no HTML rendering effect; fixture
  `38_ref_link_preserve` reflects the new contract.

## Verification

- `cargo test --release` green (216 unit + integration).
- `cargo test --release --test gfm_spec -- --ignored gfm_spec_full`: 64 unique failing cases.
- New proptest `reference_resolver_round_trips` exercises random `(label, dest, title, kind)` triples through format →
  reparse → format and HTML round-trip; 256 cases per run.
