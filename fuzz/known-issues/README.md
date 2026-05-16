# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs that need design
discussion before they land. They are kept here so the fuzzer's seed
corpus carries the minimised reproducer; do not add them to
`tests/regressions/` (the regression suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
(or `fuzz_<hash>.idem.in` if the fix is idempotence-only — see
`tests/regressions.rs`) and delete the matching entry from this
README.

## (empty)

There are no parked known-issues at the moment. The previous occupant
(`idempotence-formfeed-paragraph-resplit.in`) was promoted to
`tests/regressions/fuzz_formfeed_paragraph_resplit.idem.in` after the
paragraph-safety state machine was made exhaustive over its four
line-start contexts (the missing `after_break && !prev_line_had_text`
branch is what produced the resplit). The fix is in
`src/cm/block/paragraph.rs::apply_paragraph_safety`.
