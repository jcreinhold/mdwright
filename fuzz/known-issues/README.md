## Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs that need design
discussion before they land. They are kept here so the fuzzer's seed
corpus carries the minimised reproducer; do not add them to
`tests/regressions/` (the regression suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
(or `fuzz_<hash>.idem.in` if the fix is idempotence-only — see
`tests/regressions.rs`) and delete the matching entry from this
README.

### `html-emphasis-style-normalisation.in`

Bytes (3): `_ * _`.

- Source `_*_` → `<p><em>*</em></p>` (pulldown sees `_…_` emphasis
  around a literal `*`).
- Formatter rewrites to `\***` (italic-style canonicalisation +
  escape sieve flag): `<p>***</p>` (three literal asterisks, no
  emphasis).
- HTML diverges → `fuzz_parse_format` rejects.

The structural prerequisite (canonicalise at parse, not at emit —
`Document` owns a `Source`) has landed, and the NUL-flavoured
sibling (`idempotence-nul-emphasis-escape.in`) is fixed and promoted
to `tests/regressions/fuzz_nul_emphasis.idem.in`. The remaining
issue here is a focused emphasis-policy problem: an emphasis run
whose body or surrounding context would change pulldown's flanking
decision under delimiter rewriting (NUL was one trigger; `_…*…_` is
another) must keep its source delimiter rather than canonicalising
to the configured `ItalicStyle`. The "would change pulldown's
decision" predicate is the hard part — naive implementations invoke
pulldown twice; with the `Source`-owned `Document` it now becomes
tractable because the formatter has `(Source, Ir)` in hand and can
inspect bytes-as-the-parser-saw-them cheaply.

Reproducer:

- `cargo +nightly fuzz run fuzz_parse_format fuzz/known-issues/html-emphasis-style-normalisation.in`
