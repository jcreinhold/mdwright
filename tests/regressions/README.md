<!-- not a regression input; just documentation. -->

# Property-test regression inputs

Each `*.in` file in this directory is a minimal counterexample shrunk from one of the `tests/properties.rs` proptest
cases (or the corpus walk). Header HTML comment on each file names the property and the date the input was added.

`tests/regressions.rs` walks this directory and enforces idempotence on every `.in` it finds. The `.in` suffix (matching
the `tests/golden_*/*.in` convention) is load-bearing: the project's `mdformat` pre-commit hook globs `*.md`, which
would canonicalise the very inputs we want to preserve.

`pending/` holds counterexamples deferred to a later session. The driver does not assert against them; they exist for
documentation and reproduction. Notes are kept under `.notes` for the same reason.
