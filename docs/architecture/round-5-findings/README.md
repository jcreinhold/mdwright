# Round-5 fuzz findings — structural-emit synth vs source-bytes

## Status

Two clusters identified from the `fuzz_parse_format` corpus on 2026-05-18.
Both reproduce on `4ab8fe6`. Both share one root cause: a `.pretty()`
method synthesises Markdown bytes from IR fields instead of emitting
the construct's source bytes verbatim. The IR is a lossy projection
that drops bytes the formatter needs to round-trip.

The fix is the type-level refactor planned at
[`consider-the-prompt-groovy-bachman.md`](../../../../../.claude/plans/consider-the-prompt-groovy-bachman.md):
restructure block `.pretty()` so its return type is a sequence of
`RenderPiece<'src>`, with `Verbatim(&'src str)` the only way to emit
construct-bytes and a small audited `SeparatorKind` enum for the
joiners. The synth-from-IR path becomes structurally unrepresentable.

After Phase C / Phase D land, both clusters move to
`tests/regressions/fuzz_<hash>.in`; this directory is deleted (or kept
as a historical note).

## Cluster A — ATX heading trailing-hash decoration loss

**Reproducer:** [`fuzz/artifacts/fuzz_parse_format/crash-e395e72f…`](../../../fuzz/artifacts/fuzz_parse_format/).
Originally documented at [`round-4-findings/02-heading-trailing-hash.in`](../round-4-findings/02-heading-trailing-hash.in).

**Bytes:** `0x20 0x23 0x20 0x23 0x20 0x23` = option byte `0x20`
(`Wrap::Keep`, `FormatMode::Normalise`, `italic=Underscore`, no math
normalise) + payload `# # #` (5 bytes).

**Source event stream:** `Start(Heading(H1))`, `Text("#")`,
`End(Heading(H1))`. Pulldown applies CM §4.2's closing-`#`-sequence
rule: the trailing ` #` is decoration, the middle `#` is body text.

**Formatter output:** `# #` (3 bytes).

**Formatted event stream:** `Start(Heading(H1))`, `End(Heading(H1))`.
The middle `#` is now in closing-decoration position (preceded by
space, followed by EOL) and pulldown treats it as decoration. Body is
empty.

**Divergence:** `event 1: source = Text("#"); formatted = End(Heading(1))`.

**Mechanism:** `src/cm/block/heading.rs::Heading::pretty` ATX branch
(lines 286–298) synthesises `prefix = "#".repeat(level) + " "` + the
rendered inline body. The source's `# # #` shape — where the same `#`
byte plays different roles depending on its position relative to a
closing run — has no representation in the IR's `Heading { level,
style, attrs }`, so the emitter cannot reproduce it.

**Type-level fix:** `Heading::pretty` returns `[Verbatim(source_bytes
_for_heading), Separator(BlockTerminator)]`. Source `# # #` is
emitted verbatim. Pulldown re-parses to body `#` again. ✓

## Cluster B — Fenced code block source-byte loss

**Reproducer:** [`fuzz/artifacts/fuzz_parse_format/crash-3ab44ee9…`](../../../fuzz/artifacts/fuzz_parse_format/).

**Bytes:** `0x0a 0x60 0x60 0x60 0x0d 0x0d` = option byte `0x0a`
(`Wrap::At(80)`, `FormatMode::Verbatim`, no canon, no math normalise)
+ payload `` ```\r\r `` (5 bytes — three backticks then two CRs).

**Source event stream:** `Start(CodeBlock fenced=true)`,
`VerbatimText("\n")`, `End(CodeBlock)`. After CM §2.1 CR→LF
normalisation the payload is `` ```\n\n ``: opener `` ``` `` +
terminator `\n` + body `\n` (unclosed fenced code block).

**Formatter output:** `` ```\n `` (4 bytes).

**Formatted event stream:** `Start(CodeBlock fenced=true)`,
`End(CodeBlock)`. Output is `` ``` `` + LF, no body content. Pulldown
parses as opener + terminator + empty unclosed body.

**Divergence:** `event 1: source = VerbatimText("\n"); formatted =
End(CodeBlock)`.

**Mechanism:** `src/cm/block/code.rs::FencedCodeBlock::pretty` (lines
81–111) synthesises `fence_string` + `open` + `hard_line()` + `tail`
+ `fence_str` + `hard_line()`. The `body` it concatenates comes from
`self.body`, which is trimmed during IR construction
(`trim_end_matches('\n')` at line 82). A source body that's "one
trailing LF" trims to empty; the emitter then emits opener + LF + no
body + closer + LF, dropping the body LF.

**Type-level fix:** `FencedCodeBlock::pretty` returns
`[Verbatim(source_bytes_for_block), Separator(BlockTerminator)]`.
Source bytes go through the document-level `normalize_line_endings_lf`
once and reach pulldown unchanged in structural shape. Body is
preserved.

## Corpus + minimised-from cleanup

- `docs/architecture/round-4-findings/02-heading-trailing-hash.in`
  duplicates cluster A. Delete in Phase F.
- `docs/architecture/round-4-findings/minimized-from-*` — all 6 files
  no longer reproduce on `4ab8fe6` (verified by running each through
  `cargo +nightly fuzz run fuzz_parse_format` and
  `cargo +nightly fuzz run fuzz_idempotence`; both report no crash).
  These were libFuzzer minimisations from the prompt-54 cluster and
  the underlying bugs were fixed in that prompt's structural-emit
  audit. Delete in Phase F.
- `docs/architecture/round-4-findings/01-empty-list-item-marker-cascade.in`
  was documented as a separate cluster in prompt 54. Still present;
  needs verification after Phase C / Phase D — the structural fix
  for clusters A and B is hypothesised to also fix this one (same
  root cause). Verify in Phase D.

## Why earlier defences missed this

- `Document::format_validated` checks **idempotence-on-mode**
  (`format(format(s)) == format(s)`) per
  [`project_mdwright_phase4_math_render.md`](../../../../../.claude/projects/-Users-jcreinhold-Code-mdwright/memory/project_mdwright_phase4_math_render.md),
  not source-vs-formatted equivalence. Both clusters above are
  idempotent (`format("# #") == "# #"`); the gate accepts them.
- The fuzz `fuzz_parse_format` target uses the stricter
  source-vs-formatted property and surfaces the bugs.
- `tests/properties.rs` matrices at the document level; per-construct
  edge cases like a 5-byte ATX heading or a 5-byte unclosed fence
  don't appear unless the generator emits exactly that shape.
- `gfm_spec_coverage` snapshot pins spec-listed inputs; CM §4.2 does
  not include trailing-#-with-empty-formatted-body as a snapshot
  case.

The structural gap (no type prevents synth-from-IR drift) plus the
runtime gate's relaxation to idempotence created a hole the fuzz
oracle was the only thing watching.
