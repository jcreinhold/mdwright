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

## html-pi-leading-whitespace.in

Bytes (2): `< ?`.

`render_html("<?")` returns `" <?"` (with a leading space — pulldown
inserts whitespace around inline HTML processing-instruction starts
in some boundary cases). The formatter emits the input verbatim
(`<?`), but `render_html` of the formatted output returns `"<?"` (no
leading space). HTML diverges → `fuzz_parse_format` rejects.

Bug class: HTML-equivalence is sensitive to pulldown's
whitespace-around-inline-HTML quirks that the formatter doesn't model
explicitly. The fix needs either (a) the formatter to emit canonical
whitespace such that re-render matches the source render, or
(b) tighter inline-HTML detection in the formatter so the
representation matches pulldown's tokenisation choice. Likely
requires a deep look at `src/cm/inline/html.rs` and
`src/format/block.rs::root_verbatim_safe` for unfinished PI / CDATA
handling.

Reproducer: `cargo +nightly fuzz run fuzz_parse_format fuzz/known-issues/html-pi-leading-whitespace.in`.

## idempotence-nul-emphasis-escape.in

Bytes (16): `\n \x04 \0\0\0\0\0\0\0\0 * _ \0 _ ~` (with a leading `\n`
option-byte consumed by `fuzz_idempotence` as
`Wrap::At(80)` / `FormatMode::Normalise` / `math.normalise=false`).

The body's `*_\0_~` (asterisk + underscore + NUL + underscore + tilde)
emphasis-candidate run interacts with mdwright's escape sieve: pass
1 normalises the emphasis delimiters to one shape (`**\0*~`), pass 2
sees that shape and inserts backslash escapes (`*\*\0\*~`),
non-idempotent.

Bug class: emphasis-run resolution + escape sieve interact
non-deterministically on inputs with NUL bytes between emphasis
markers. CM §2.3 says NUL is replaced with U+FFFD pre-parse — pulldown
does this internally — so re-parse sees the FFFD bytes, not NUL. But
the formatter slices source bytes that still contain NUL, and the
inline emphasis IR encodes delimiter choices that may not survive
re-tokenisation around FFFD.

Two fix shapes:
   (a) Canonicalise NUL → FFFD at `Document::parse` so source and
       IR-derived bytes agree, removing the discrepancy at the
       source-byte layer.
   (b) Make the emphasis IR's emit decisions consult only data that
       survives re-parse (similar to the Bug B fix: pure function of
       typed-IR fields, not of escape-sieve-tweaked rendered bytes).

Reproducer: `cargo +nightly fuzz run fuzz_idempotence fuzz/known-issues/idempotence-nul-emphasis-escape.in`.
