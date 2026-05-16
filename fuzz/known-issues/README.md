# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs (GFM-spec regressions,
behaviour changes) that need design discussion before they land. They
are kept here so the fuzzer's seed corpus carries the minimised
reproducer; do not add them to `tests/regressions/` (the regression
suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
and delete the matching entry from this README.

## idempotence-setext-underline-on-soft-break.in

Bytes: `L 0 B \n \t \t = \t` (8 bytes).

`pulldown_cmark` parses this as a paragraph with two text nodes
joined by a soft break: `"L0B"` + soft-break + `"="`. After format,
the soft break renders as a hard line under `Wrap::Keep` (the
default), producing:

```
L0B
=
```

That output re-parses as a setext H1 — pulldown treats a bare `=`
line following paragraph text as a level-1 underline. The second
format then emits an ATX heading (`# L0B\n`), so
`format(parse(format(parse(s)))) ≠ format(parse(s))`.

The targeted fix is to escape `=` (and `-`) at the start of any
paragraph continuation line. A first attempt at this
(commit-window 2026-05-16) widened the escape pass to treat
`Doc::Line` the same as `Doc::HardLine` and to escape pure `=`/`-`
runs, but the broader change introduced two `gfm_spec` regressions
that we have not yet triaged. The right fix probably needs to be
narrower — only escape when the *previous* line could form a setext
underline pair with this one (i.e. previous line is paragraph text,
current line is a pure underline run, and the wrap mode would
preserve the line break).
