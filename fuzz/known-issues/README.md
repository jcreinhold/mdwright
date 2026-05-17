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

*Currently empty.* Last clean: 2026-05-16, after the structural
emit-safety landing (`src/format/emit_safety.rs`) closed bug class A
(emphasis-flanking instability) by gating every emphasis/strong emit
through a per-construct fallback ladder: try the configured style →
escape body bytes that became flanking-active → fall back to source
verbatim. The previously-parked `_*_` fixture is now
`tests/regressions/fuzz_emphasis_style_normalisation.in`.
