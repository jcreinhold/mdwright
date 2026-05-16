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

## idempotence-emphasis-strikethrough-escape-drift.in

Bytes: `Hww$\0***$~\0B*~~B~` (17 bytes).

The output gains an extra `\*` escape on the second format pass
compared with the first (`B*~~B~~` → `B\*~~B~~`). One single `*`
inside text adjacent to a `~~` strikethrough boundary gets escaped
on reformat in one direction but not the other, breaking
idempotence.

This is a **different bug class** from the typed-constructor
families fixed so far (paragraph body, code span). It involves the
emphasis emitter's decisions around `*` characters that sit next to
strikethrough (`~~`) delimiter runs — likely the run-resolution
logic in `cm/inline/emphasis.rs` doesn't reach the same fixed point
on the two passes because the surrounding context differs after
the first format adds escapes.

Fix shape: as with paragraph body and code span, the typed inline
constructor (`EmphasisRun` / `StrongRun`) should encode "what I emit
reparses to me" as a debug-time round-trip self-check. The current
constructor decides delimiter style and escapes per-pass; the
invariant the fuzz find shows must hold is that the decision is a
fixed point of (parse → emit). Audit `resolve()` in the emphasis
module.
