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

## idempotence-formfeed-paragraph-resplit.in

Bytes (10): `K \n \f \n + \n \f * * B` (the `\f` bytes are form-feed, U+000C).

Once collapses the form-feed "line" between `K` and `+` into a
blank-line separator, leaving `+\n\f**B` as one paragraph; twice
reparses the resulting `+\n` as a bullet-list marker for an empty
item (default bullet style `- `) and inserts an extra blank line:

```
once : K\n\n+\n\f**B\n
twice: K\n\n- \n\n\f**B\n
```

Bug class differs from the lone-`*`-on-first-line family closed in
`5d63f2a` / the helper merge: here the `+` is on a *continuation*
line of a paragraph on the first format pass, so the line-start
escape doesn't fire; on the second pass the form-feed-separator
context that protected it on the first pass is gone, so the same
`+\n` is now a list marker. Either:

- the form-feed-only "line" must be classified by mdwright the
  same way pulldown classifies it (so once and twice agree on
  block boundaries), or
- the safety pass needs to escape `+`/`-`/`*` at the start of any
  paragraph-continuation line that becomes line-start-after-blank
  in the emitted output (broader than the current
  `escape_for_paragraph_interrupt` set, which by CM §5.3 declines
  empty markers).

Reproducer: `cargo +nightly fuzz run fuzz_idempotence fuzz/known-issues/idempotence-formfeed-paragraph-resplit.in`.
