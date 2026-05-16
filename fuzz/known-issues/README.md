# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs (GFM-spec regressions,
behaviour changes) that need design discussion before they land. They
are kept here so the fuzzer's seed corpus carries the minimised
reproducer; do not add them to `tests/regressions/` (the regression
suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
and delete the matching entry from this README.

## idempotence-blank-line-drift.in

Bytes: `:\x01\n\x0c\n\n\x0c\x0c 1\x00\x00` (11 bytes).

`once = format(parse(s))` emits one extra `\n` between two paragraphs
(`"…\n\n\n…"` vs the expected `"…\n\n…"`). The second format
collapses the extra blank to one, so the second pass differs from the
first and idempotence fails.

This is a **different bug class** from the paragraph-continuation
re-tokenisation family that the typed `ParagraphBody` constructor now
makes unrepresentable. The cause sits in document-root block
separation (likely an off-by-one in the inter-block hard-line count
when a block ends with content containing trailing form-feed
characters, or a normalisation that runs only once). It does not
involve the paragraph escape pass and cannot be fixed there.

Triage path: bisect which block boundary inserts the extra hard-line
on the first format; check `src/format/block.rs` document-root
emission and the per-typed-block `pretty()` terminators.
