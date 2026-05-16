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

## Emphasis-style normalisation breaks pulldown's flanking decisions

Two fixtures, **same class**. Both stem from the formatter's
`ItalicStyle::Asterisk` (default) rewriting `_…_` to `*…*` — which
exposes the rewritten asterisks to the surrounding context in ways
pulldown's emphasis-flanking rule then parses differently than the
source.

### `html-emphasis-style-normalisation.in`

Bytes (3): `_ * _`.
- Source `_*_` → `<p><em>*</em></p>` (pulldown sees `_…_` emphasis
  around a literal `*`).
- Formatter rewrites to `\***` (italic-style canonicalisation +
  escape sieve flag): `<p>***</p>` (three literal asterisks, no
  emphasis).
- HTML diverges → `fuzz_parse_format` rejects.

### `idempotence-nul-emphasis-escape.in`

Bytes (15): `\x04\0\0\0\0\0\0\0\0*_\0_~` (with a leading `\n`
option-byte consumed by `fuzz_idempotence` as
`Wrap::At(80)` / `FormatMode::Normalise` / `math.normalise=false`).

The body's `*_\0_~` rewrites to `**\0*~`; pass 2's paragraph-safety
sieve treats the now-exposed inner `*` as a paragraph-interrupter,
inserting `\*` → `*\*\0\*~`. Non-idempotent.

### Investigation update (the dead end)

The obvious structural candidate for the NUL variant — extending
`src/format/doc.rs::text` to also canonicalise NUL → U+FFFD per CM
§2.3 — was tried and reverted because pulldown's emphasis-flanking
rule treats NUL and FFFD differently:

- `*_\0_~` parses as `Text("*") + Emphasis(Text("\0")) + Text("~")`
  — pulldown sees `_\0_` as an emphasis run.
- `**\0*~` parses as `Text("*") + Emphasis(Text("\0")) + Text("~")`
  — same shape (single asterisks paired around NUL).
- `**\u{FFFD}*~` parses as five separate `Text` events, **no
  emphasis** — FFFD's 3-byte UTF-8 sequence sits where the flanking
  rule expected a 1-byte character, breaking the run.

So substituting NUL → FFFD at the formatter's emit layer would
change pulldown's re-parse structure relative to the source, making
the divergence *worse*. The `text()` doc comment in
`src/format/doc.rs` records this so the next maintainer doesn't
repeat the experiment.

### Viable fix shapes

(a) **Canonicalise at parse, not at emit.** A `Document::parse`
    canonicalising the source `String` once (CM §2.1 line endings +
    CM §2.3 NUL → FFFD) would make the source and the next parse
    agree, because the substitution applies to both. This implies
    owning the canonical buffer inside `Document` (the
    lifetime-elimination refactor: `Document<'a>` → `Document`),
    which is much larger surgery than the parked-bug count justifies
    right now.

(b) **Conservatise emphasis-style normalisation.** When an emphasis
    run's body or surrounding context would change pulldown's
    flanking decision under delimiter rewriting (NUL is one trigger;
    `_…*…_` is another), keep the source delimiter rather than
    rewriting to the configured `ItalicStyle`. This is a
    pure-function-of-typed-IR rule similar to the Bug B
    setext-vs-ATX fix; the "would change pulldown's decision"
    predicate is the hard part — naive implementations end up
    invoking pulldown twice.

Reproducers:
- `cargo +nightly fuzz run fuzz_parse_format fuzz/known-issues/html-emphasis-style-normalisation.in`
- `cargo +nightly fuzz run fuzz_idempotence fuzz/known-issues/idempotence-nul-emphasis-escape.in`
