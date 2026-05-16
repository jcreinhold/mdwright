# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs (GFM-spec regressions,
behaviour changes) that need design discussion before they land. They
are kept here so the fuzzer's seed corpus carries the minimised
reproducer; do not add them to `tests/regressions/` (the regression
suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
and delete the matching entry from this README.

## idempotence-tab-strip-becomes-blockquote.in

Bytes: `: \t \x01 \x02 \0 \x04 \n \t > > > >` (12 bytes).

`pulldown_cmark` parses this as a paragraph with two lines, the
second of which starts with a tab + `>>>>`. mdwright's formatter
strips the leading tab when emitting paragraph continuation lines,
so the round-trip produces:

```
:\t\x01\x02\0\x04
>>>>
```

The `>>>>` line, now flush at column 0 with paragraph text above it,
re-parses as four nested blockquotes — pulldown sees `> > > >` shape.
On second format, the document becomes paragraph + blockquote stack,
and `format(parse(format(parse(s)))) ≠ format(parse(s))`.

This is the same family of bug as the now-fixed
`fuzz_236b414f.in` (setext-underline after soft break): the formatter
detoxifies whitespace on continuation lines, and the result is then
re-tokenised as a different block. A clean fix would generalise the
narrow `escape_setext_underline` helper in
`src/cm/block/paragraph.rs` to cover `>` (blockquote) and the rest
of the block-leader set when `after_break && prev_line_had_text`,
guarded carefully so the GFM-spec snapshot does not regress (a first
attempt at the broad form did regress 2 cases — see git log for
the setext fix). Doing that requires triaging the specific corpus
cases the broad escape touches.
