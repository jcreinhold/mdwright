# Changelog

All notable changes to mdwright are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions
follow [SemVer](https://semver.org/spec/v2.0.0.html).

> Note on the version jump: 0.1.0 → 0.3.0 skips 0.2.0 deliberately.
> An interim 0.2.0 was reserved for the unreleased pre-Phase-R baseline
> (tagged in git as `phase-r-baseline-pre-tracing`) but was never cut;
> the spec-alignment redesign ships as 0.3.0 to keep the released
> sequence in step with the in-repo Phase-R prompt block.

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
