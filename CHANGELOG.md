# Changelog

All notable changes to mdwright are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow [SemVer](https://semver.org/spec/v2.0.0.html).

> Note on the version jump: 0.1.0 → 0.3.0 skips 0.2.0 deliberately.
> An interim 0.2.0 was reserved for the unreleased pre-Phase-R baseline
> (tagged in git as `phase-r-baseline-pre-tracing`) but was never cut;
> the spec-alignment redesign ships as 0.3.0 to keep the released
> sequence in step with the in-repo Phase-R prompt block.

## Unreleased

### Added
- Coverage-guided fuzz harness at [`fuzz/`](./fuzz) with three
  targets: `fuzz_parse_format`, `fuzz_idempotence`, `fuzz_lint`.
  See [README §Safety](./README.md#safety).
- `--max-input-bytes` global CLI flag (default 10 MB) caps the size
  of any single file or stdin payload. Pass `0` to disable.
- `tests/discover_symlink_loop.rs` pins the symlink-handling
  contract; `discover_markdown` does not follow symlinks and so is
  immune to symlink loops.
- `SECURITY.md` disclosure template.

### Changed
- `src/format/wrap.rs` bounds the Knuth-Plass-lite DP: paragraphs
  exceeding 100 000 boxes skip the DP and emit verbatim; a 100 ms
  time budget guards against generators we did not anticipate.
- `src/cm/block/heading.rs` falls back to ATX form when a setext
  body would re-parse as a different block (e.g. `*`, `>`, `#`,
  digits, fenced-code leaders) and keeps setext for multi-line
  bodies (which ATX cannot represent). Fixes two fuzz-found
  idempotence regressions.
- `src/cm/block/list.rs` emits a hard-line for an empty list item
  so adjacent empty items do not collapse into a thematic-break
  shape on re-parse. Fixes one fuzz-found idempotence regression
  and resolves six GFM-spec list-item snapshot cases.

- `src/cm/block/paragraph.rs` introduces a typed `ParagraphBody<'a>`
  newtype whose single constructor (`from_inline`) runs the
  line-start safety pass. `Paragraph::pretty` and
  `list.rs::render_item_body` are switched to it; the previously-public
  `escape_paragraph_line_starts` is gone. This makes the
  "paragraph continuation line re-tokenises as a different block on
  reparse" bug class **unrepresentable** — every paragraph body is,
  by construction, paragraph-safe; adding coverage for a new
  interrupter character is a one-line edit inside one helper rather
  than per-caller-path discipline. The safety pass uses a strict
  CM-correct paragraph-interrupter set
  (`escape_for_paragraph_interrupt`) for the soft-break case: only
  `>`, ATX with required space, bullet-with-content, ordered list at
  start=1, fenced code, thematic break. Closes the previously-deferred
  `fuzz_236b414f.in` (setext underline) AND `fuzz_09a8d6b1.in` (tab
  + `>>>>` re-parsing as nested blockquote). Resolves 5 more
  GFM-spec snapshot cases (ATX heading 40 idem; Block quotes 216
  idem; Task list items 280 html+ast; Lists 292 html+ast); leaves a
  pre-existing nested-list-indent bug visible as 280 idempotence.
- `src/ir.rs::split_frontmatter` now requires the candidate body to
  contain at least one `key:` (YAML) or `key =` (TOML) line. Without
  this gate, a document whose first line is a thematic break (`---`)
  and whose body contains another thematic break is misidentified
  as YAML frontmatter, silently dropping everything between. Caught
  by proptest after the thematic-break normalisation made the round
  trip reach the shape. New regression at
  `tests/regressions/frontmatter_false_positive.in`.
- `src/cm/block/paragraph.rs::flatten` and
  `src/cm/inline/link.rs::flatten_body_doc` are now iterative —
  no stack risk on deeply nested `Doc::Concat`.

### Known issues
- One new (different-family) fuzz find: a paragraph break followed
  by certain control-character payloads yields one extra blank line
  on the first format that collapses on the second, so
  `format ∘ format ≠ format`. Outside the paragraph-continuation
  bug class fixed above; reproducer parked at
  [`fuzz/known-issues/idempotence-blank-line-drift.in`](./fuzz/known-issues/README.md).

## [0.3.0] — 2026-05-16 — spec-alignment redesign

### Changed
- IR is now spec-aligned: each CM/GFM construct is a typed
  Rust value whose constructor enforces well-formedness.
- The `format::*` sieve is replaced by per-construct `pretty()`
  methods on each typed value, dispatched through
  `TypedBlock::pretty`. ~1,500 LOC deleted net.
- Spec conformance is a construction-time property rather than
  a 672-case runtime sieve.
- `--verbose` / `-v` count-flag controls `tracing` log level.
  Logs are silent by default; `-vvv` shows per-construct
  decisions.

### Added
- `mdwright::cm::{inline, block, refs}` typed IR modules with
  per-construct `pretty()` methods.
- Per-construct round-trip proptests; the whole-document GFM-
  spec runner is now a snapshot.
- `--mode={normalise,verbatim}` flag.
- `docs/deviations.md` — user-facing index of where the
  formatter diverges from the spec, with the snapshot /
  allowlist mechanism described.

### Removed
- The per-byte escape sieve (moved into the typed-value
  constructor in prompt 20).
- `FULL_BASELINE_FAILURES` ratchet.
- Legacy `render_*` family: `render_emphasis`, `render_strong`,
  `render_link`, `render_image`, `render_heading`,
  `render_blockquote`, `render_list`, `render_table`.
- `NodeKind::LinkReferenceDefinition`: link reference data is
  now read from the per-document `ReferenceTable` directly
  rather than synthesised as a tree node.

### Performance
- Format-only steady-state benches are **25–27 % faster** than the
  v0.2.0 sieve (`format/small`, `format/medium`, `format_wrap/keep`).
- `format_wrap/at-{80,100,120}` are 9–12 % faster.
- The end-to-end parse-plus-format path is 8–15 % slower per call
  because IR construction now does more work per pulldown event; the
  parallel CLI wall-clock metric is dominated by I/O and parse so the
  regression is not visible there. A follow-up release will close the
  parse-side gap.

### Fixed
- All 17 HTML-divergent CM/GFM spec cases.
- All 17 idempotence-failing CM/GFM spec cases.
- ≈ 100 AST-only divergences (most were pulldown-cmark text-run
  chunking and now go via the verbatim path).

## [0.1.0] — initial release

First public release. Linter only; no formatter.
