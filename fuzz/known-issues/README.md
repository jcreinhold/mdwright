# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs (GFM-spec regressions,
behaviour changes) that need design discussion before they land. They
are kept here so the fuzzer's seed corpus carries the minimised
reproducer; do not add them to `tests/regressions/` (the regression
suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
(or `fuzz_<hash>.idem.in` if the fix is idempotence-only — see
`tests/regressions.rs`) and delete the matching entry from this
README.

## idempotence-code-span-padding-grows.in

Bytes: `\0\n` `` ` `` `\u{5a5}\0\n` `` ` `` `\u{5a5}`` ` `` ` ``  (16 bytes).

Each format pass widens the content of an inline code span. Source
has `Code(" ")` (one space inside a `` `` `` … `` `` `` span); first
format produces `Code("   ")` (three spaces); second produces
`Code("     ")` (five). The growth is `+2 spaces per format`.

Cause is in the inline code-span emitter (`src/cm/inline/code.rs`
or thereabouts), which applies CM §6.1's leading/trailing-space
padding rule when the content starts or ends with a backtick — but
the emitter doesn't *first* normalise the content to remove any
padding pulldown already added on its own pass. Padding accumulates.

This is a **different bug class** from the paragraph-body invariants
that `ParagraphBody::from_inline` enforces. The fix is local to the
inline code-span constructor: normalise content (strip exactly one
leading/trailing space when both ends touch a backtick) before
deciding whether to re-add padding. Make padding idempotent at the
typed-construct level — the same Phase R / `ParagraphBody`
discipline applied to code spans.
