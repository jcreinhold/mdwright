# GFM spec test data

`spec.txt` is vendored verbatim from <https://github.com/github/cmark-gfm>, the same suite comrak and mdformat-gfm test
against.

- Source: `https://raw.githubusercontent.com/github/cmark-gfm/master/test/spec.txt`
- Pinned upstream commit: `587a12bb54d95ac37241377e6ddc93ea0e45439b` (2023-07-21)
- License: CC-BY-SA 4.0 (per the spec frontmatter).
- Vendored on: 2026-05-14.

To refresh, run from the repo root:

```
curl -fsSL \
  https://raw.githubusercontent.com/github/cmark-gfm/master/test/spec.txt \
  -o tools/mdwright/tests/gfm-spec/spec.txt
```

Update the pinned-commit line above when refreshing.

## Format

Cases are delimited by 32-backtick fences with an `example` tag, optionally followed by a class (`autolink`, `disabled`,
`strikethrough`, `table`, `tagfilter`). Tabs in the source are escaped as `→` (U+2192) and decoded back to `\t` at load
time. Each example has a Markdown source, a single `.` separator line, and the expected HTML output. The runner does
**not** compare against the expected HTML — it asserts our formatter's idempotence and AST/HTML equivalence against the
*parsed* source. The expected HTML is therefore informational here.

## Bare URL autolinks

mdwright does not enable pulldown-cmark's GFM bare-URL autolink extension. Bare URLs in source parse to plain `Text`
nodes and round-trip verbatim through the formatter. The handful of spec cases under `example autolink` that test
bare-URL recognition land in `known-mismatches.txt` because our parser refuses to treat them as autolinks — a deliberate
choice that keeps the formatter's output stable for prose corpora.

## Allowlist

`known-mismatches.txt` lists cases the runner unconditionally skips. Each line has the form `<case-number> <reason>`.
Reserved for cases the formatter genuinely cannot round-trip (e.g. source-form quirks no normalising formatter would
preserve). Empty at landing.

## Baseline-mode

The exhaustive sweep `gfm_spec_full` (run with `cargo test --release -- --ignored gfm_spec_full`) does **not** require
zero failures. It asserts the failing-case count stays at or below `FULL_BASELINE_FAILURES` in `tests/gfm_spec.rs`. The
current baseline reflects mdwright's formatter as of Phase 3 landing — substantial gaps remain (raw-HTML round-tripping,
list-item idempotence, edge cases in emphasis nesting, bare-URL autolinks under the GFM extension). Future sessions
should tighten the baseline toward zero.

The default `cargo test --release` runs `gfm_spec_fast`: a small hand-curated subset of cases known to pass under both
invariants (HTML equivalence and AST-event equivalence). New entries to this list must currently pass; the fast subset
is the regression sentinel.
