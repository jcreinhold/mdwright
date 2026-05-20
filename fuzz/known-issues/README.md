# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have not been fixed: the fix has tradeoffs that need design
discussion before it lands. They live here so the fuzzer's seed corpus carries the minimised reproducer; do not add them
to `tests/regressions/`, where the regression suite would fail.

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in` (or `fuzz_<hash>.idem.in` if the fix is
idempotence-only; see `tests/regressions.rs`) and delete the matching entry from this README.

*Currently empty.*
