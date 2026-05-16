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

## idempotence-indented-code-line-drop.in

Bytes: `\t\r\t\0\0[\x07` (7 bytes).

Two consecutive tab-prefixed lines (separated by `\r`) form an
indented code block in CM. Once preserves both lines; twice drops
one:

```
once:  \t\n\t\0\0[\x07\n
twice: \t\0\0[\x07\n        (first \t-line gone)
```

Lives in the indented-code-block emitter
(`src/cm/block/code.rs::IndentedCodeBlock::pretty`) or the
document-root verbatim path for indented-code blocks. The bug
class: an indented code block with a "trivial" first line (only
whitespace) loses that line on re-parse / re-emit. Fix shape: as
with the other typed-construct invariants, the indented-code
emitter must preserve every line of body content, including
blank-looking ones whose `\t` prefix carries the only structural
signal.
