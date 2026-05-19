# GFM spec test data

`spec.txt` is vendored verbatim from <https://github.com/github/cmark-gfm>, the same suite comrak and mdformat-gfm test
against.

- Source: `https://raw.githubusercontent.com/github/cmark-gfm/master/test/spec.txt`
- Pinned upstream commit: `587a12bb54d95ac37241377e6ddc93ea0e45439b` (2023-07-21)
- License: CC-BY-SA 4.0 (per the spec frontmatter).
- Vendored on: 2026-05-14.

To refresh, run from the repo root:

```
curl -fsSL \
  https://raw.githubusercontent.com/github/cmark-gfm/master/test/spec.txt \
  -o crates/mdwright/tests/gfm-spec/spec.txt
```

Update the pinned-commit line above when refreshing.

## Format

Cases are delimited by 32-backtick fences with an `example` tag, optionally followed by a class (`autolink`, `disabled`,
`strikethrough`, `table`, `tagfilter`). Tabs in the source are escaped as `→` (U+2192) and decoded back to `\t` at load
time. Each example has a Markdown source, a single `.` separator line, and the expected HTML output. The runner does
**not** compare against the expected HTML. It asserts our formatter's idempotence and AST/HTML equivalence against the
*parsed* source. The expected HTML is therefore informational here.

## GFM autolinks and tagfiltering

pulldown-cmark 0.13 does not implement GFM's extended autolink or tagfilter extensions. mdwright adds a document-owned
overlay for bare URL autolinks, bare email autolinks, and tagfilter rendering so rendered HTML matches cmark-gfm for
those extension cases while the formatter still round-trips source bytes.

## Runner

`gfm_spec.rs` is a formatter round-trip harness. It parses the source side, formats it, reparses the result, and checks
idempotence plus mdwright semantic equivalence. It does **not** compare against the expected HTML embedded in
`spec.txt`; that cmark-gfm conformance role belongs to `cargo xtask parser-audit`.

Phase R replaced the ratchet with a snapshot. Two tests in `tests/gfm_spec.rs`:

- `gfm_spec_snapshot` runs every case through `parse → format → parse → format`, collects the residual `(case, kind)`
  failures *not* covered by `allowlist.toml`, and asserts byte-for-byte equality with `snapshot.txt`. Any drift,
  regression or improvement, fails CI. Regenerate after a deliberate change with:

  ```
  MDWRIGHT_UPDATE_SNAPSHOT=1 cargo test --release --test gfm_spec gfm_spec_snapshot
  ```

- `gfm_spec_coverage` prints a three-line coverage report and asserts that every spec case lands in exactly one of
  `matching`, `editorial deviation` (allowlist), or `tracked regression` (snapshot).

## Allowlist vs. snapshot

- `allowlist.toml`: *editorial deviations*. Choices we have made and intend to keep. Currently empty; adding here is a
  statement that mdwright will never round-trip that case.
- `snapshot.txt`: *tracked regressions*. Known divergences we intend to fix. Adding here is a statement that the
  current output is wrong, but recorded so we notice when it changes.

User-facing summary of both: [`docs/deviations.md`](../../docs/deviations.md).
