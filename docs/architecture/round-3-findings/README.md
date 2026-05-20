# Round-3 fuzz findings

Two reproducers from a 5-min/target fuzz verification that exposed output-decisions-consult-source-bytes (pattern #2
in [`../fuzz-history.md`](../fuzz-history.md)). Both now byte-preserve under default options; the inputs live in the
regression suite:

- `01-parse-format-nested-emphasis-with-slash.in` → `tests/regressions/fuzz_round3_nested_emphasis_slash.in`
- `02-idempotence-emphasis-strong-strikethrough.in` → `tests/regressions/fuzz_round3_multi_construct_idempotence.in`

## `_*/*_` (5 bytes)

Pulldown event stream: `Emphasis(Emphasis(Text("/")))`—that is, the same shape as GFM-spec example 470 (`*_foo_*`) with
the body byte `/`.

A predictive formatter produced `*\*/\**`, which re-parses to a single `Emphasis(Text("\\*/\\*"))`. The structural-emit
redesign emits the source bytes verbatim; the nested-IR shape survives by construction.

## `**u*~***~` (option `0x21` selects `Wrap::No`, `FormatMode::Normalise`, no math normalise)

Pulldown event stream: `Emphasis(Emphasis("u") + Text("~")) + Text("**") +
Text("~")`. The trailing `~` characters are
*not* a strikethrough pair— pulldown's pairing decision keeps them as plain text because the outer emphasis closes at
byte 5 and consumes the flanking `*`.

A predictive formatter oscillated between `**u*~*\*\*~` (pass 1) and `**u*~~\*\*\*~~` (pass 2). Pass 1's escape policy
turned the trailing `**` into `\*\*`, which changed the `~`/`~~` pairing flanking on re-parse.

The current escape policy at `src/cm/inline/escape_policy.rs::needs_emphasis_escape` requires a non-delimiter body byte
between paired delimiters before treating them as an emphasis candidate. The trailing `**` no longer gets escaped and
the input round-trips.
