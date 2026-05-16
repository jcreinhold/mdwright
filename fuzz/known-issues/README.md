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

## html-list-bullet-merge.in

Bytes (3): `+ \n -` (the `+`, LF, `-`).

Pulldown classifies `+\n-` as **two separate lists**, each with one
empty list item, because the bullet character changes between marker
lines (CM §5.2: a list ends when the marker character changes).
Source HTML: `<ul><li></li></ul><ul><li></li></ul>`.

mdwright's `ListMarkerStyle::Dash` default normalises both markers to
`-`, emitting `- \n\n- `. Pulldown re-parses that as **one list with
two empty items**: `<ul><li></li><li></li></ul>`. HTML diverges →
`fuzz_parse_format` rejects.

Bug class: stylistic bullet-character normalisation can merge
adjacent lists that the source intentionally separated by using
different markers. The fix needs a list-boundary preservation
mechanism — either (a) detect when adjacent lists in the source
would merge under the chosen marker style and either keep the
distinct markers or insert an HTML-equivalent separator, or (b)
change the default to `Preserve` and document that
`Dash`/`Asterisk`/`Plus` are unsafe for documents using bullet-style
changes as list boundaries.

Reproducer: `cargo +nightly fuzz run fuzz_parse_format fuzz/known-issues/html-list-bullet-merge.in`.

## idempotence-cr-in-setext-body.in

Bytes (9): `\x19 \x19 \r \t \0 \n = \n` (with a leading `\x19`
option-byte prefix consumed by `fuzz_idempotence` as
`Wrap::At(80)`, normalise mode, math.normalise=false).

The body of the setext heading contains a CR byte. After the recent
heading-body-source-verbatim emit (commit "heading: decide setext-vs-
ATX from source bytes"), mdwright copies the CR through to its
rendered output. The post-render `normalize_line_endings_lf` then
converts CR → LF, changing the body byte length, which changes the
underline width on the next pass. Pass 1 emits `=====` (5 chars
matching pre-LF-normalisation body); pass 2 sees a different body
length and emits `===` (3 chars). Non-idempotent.

Bug class: the document-boundary line-ending normaliser
(`normalize_line_endings_lf` at `src/format/mod.rs:39`) does not
distinguish CR-as-line-terminator from CR-as-content. The setext body
verbatim path is the first emit site that legitimately carries
content CR through render. Two fix shapes:
   (a) Normalise CR → LF inside body source bytes *before* the
       verbatim emit, so the post-render normaliser is a no-op on
       that span (downside: changes source bytes unconditionally,
       though CR-as-content is exceedingly rare).
   (b) Distinguish content CR from terminator CR at the IR layer,
       so the post-render pass only touches terminators.

Reproducer: `cargo +nightly fuzz run fuzz_idempotence fuzz/known-issues/idempotence-cr-in-setext-body.in`.
