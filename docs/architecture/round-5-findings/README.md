# Round-5 fuzz findings

Two clusters from the `fuzz_parse_format` corpus that share a single root cause: a `.pretty()` method synthesises
Markdown bytes from IR fields instead of emitting the construct's source bytes verbatim. The IR is a lossy projection
that drops bytes the formatter needs to round-trip.

The fix is a type-level refactor: block `.pretty()` returns a sequence of `RenderPiece<'src>`, with
`Verbatim(&'src str)` the only way to emit construct-bytes and a small audited `SeparatorKind` enum for joiners. The
synth-from-IR path becomes structurally unrepresentable.

## Cluster A—ATX heading trailing-hash decoration loss

Reproducer: `fuzz/artifacts/fuzz_parse_format/crash-e395e72f…`. Also documented at
[`../round-4-findings/02-heading-trailing-hash.in`](../round-4-findings/).

Bytes: option `0x20` (`Wrap::Keep`, `FormatMode::Normalise`, `italic=Underscore`, no math normalise) + payload `# # #`.

| Stage     | Event stream                                              | Bytes  |
| --------- | --------------------------------------------------------- | ------ |
| Source    | `Start(Heading H1), Text("#"), End(Heading H1)`           | `# # #` |
| Formatted | `Start(Heading H1), End(Heading H1)`                      | `# #`  |

Pulldown applies CM §4.2's closing-hash-sequence rule: in `# # #`, the trailing ` #` is decoration; the middle `#` is
body text. After mdwright emits `# #`, the middle `#` is in closing-decoration position (preceded by space, followed by
EOL) and pulldown reads it as decoration, leaving the body empty.

Cause: `src/cm/block/heading.rs::Heading::pretty` ATX branch synthesises `"#".repeat(level) + " "` + the rendered
inline body. The source's `# # #` shape—where the same `#` byte plays different roles depending on position—has no
representation in the IR's `Heading { level, style, attrs }`, so the emitter cannot reproduce it.

Fix shape: `Heading::pretty` returns `[Verbatim(source_bytes_for_heading),
Separator(BlockTerminator)]`. Source `# # #`
emits verbatim; pulldown re-parses to body `#`.

## Cluster B—Fenced code-block source-byte loss

Reproducer: `fuzz/artifacts/fuzz_parse_format/crash-3ab44ee9…`.

Bytes: option `0x0a` (`Wrap::At(80)`, `FormatMode::Verbatim`, no canon, no math normalise) + payload `` ```\r\r ``
(three backticks then two CRs; becomes `` ```\n\n `` after CM §2.1 CR→LF normalisation: opener `` ``` `` + terminator
`\n` + body `\n`, unclosed fence).

| Stage     | Event stream                                                  | Bytes      |
| --------- | ------------------------------------------------------------- | ---------- |
| Source    | `Start(CodeBlock fenced), VerbatimText("\n"), End(CodeBlock)` | `` ```\n\n `` |
| Formatted | `Start(CodeBlock fenced), End(CodeBlock)`                     | `` ```\n ``  |

Cause: `src/cm/block/code.rs::FencedCodeBlock::pretty` synthesises
`fence_string + open + hard_line() + tail + fence_str + hard_line()`. The body comes from `self.body`, which is trimmed
during IR construction (`trim_end_matches('\n')`). A source body that is "one trailing LF" trims to empty; the emitter
then emits opener + LF + closer + LF, dropping the body LF.

Fix shape: `FencedCodeBlock::pretty` returns `[Verbatim(source_bytes_for_block), Separator(BlockTerminator)]`. Source
bytes pass through `normalize_line_endings_lf` once and reach pulldown unchanged.

## Why earlier defences missed these

- `Document::format_validated` checks idempotence-on-mode (`format(format(s)) == format(s)`), not source-vs-formatted
  equivalence. Both clusters are idempotent (`format("# #") == "# #"`); the gate accepts them.
- `fuzz_parse_format` uses the stricter source-vs-formatted property and surfaces the bugs.
- `tests/properties.rs` matrices at the document level; per-construct edge cases like a 5-byte ATX heading or a 5-byte
  unclosed fence don't appear unless the generator emits exactly that shape.
- `gfm_spec_coverage` pins spec-listed inputs; CM §4.2 does not include trailing-hash-with-empty-formatted-body as a
  snapshot case.

The structural gap (no type prevents synth-from-IR drift) plus the runtime gate's relaxation to idempotence created a
hole the fuzz oracle was the only thing watching.
