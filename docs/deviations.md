# mdwright spec deviations

The mdwright formatter targets the GFM 0.29-gfm spec
(`tests/gfm-spec/spec.txt`, vendored from cmark-gfm). Every example is
exercised by `tests/gfm_spec.rs` as a `parse → format → parse → format`
round-trip and compared against the source HTML and the normalised
event stream.

This document is the user-facing index of where mdwright currently
**does not** byte-for-byte round-trip the spec. It is split into two
parts because the underlying mechanism does:

- **Editorial deviations** — choices we have made and intend to keep.
  Curated in `tests/gfm-spec/allowlist.toml`. Each entry has a one-line
  rationale and a pointer to where the decision is documented.
- **Tracked regressions** — known divergences that we intend to fix.
  Recorded in `tests/gfm-spec/snapshot.txt`. The snapshot is asserted
  byte-for-byte, so any drift — regression *or* improvement — fails CI
  and forces a deliberate update.

The `gfm_spec_coverage` test prints the live count for both groups; the
numbers below are a snapshot of the current main branch.

## Coverage

| Bucket                 | Examples |
| ---------------------- | -------- |
| Spec examples total    | 672      |
| Matching               | 615      |
| Editorial deviations   | 0        |
| Tracked regressions    | 57       |

A *case* may fail more than one comparison kind (`html`, `ast`,
`idempotence`); the snapshot file is keyed by `(case, kind)` and
currently lists 70 entries across 57 distinct cases.

## Editorial deviations

None yet. `allowlist.toml` ships empty at v0.3.0: every divergence
above is in the *tracked-regression* bucket, not the editorial bucket.
We are deliberately conservative — adding to the allowlist is a
statement that mdwright will never round-trip that case. Phase R put
the *mechanism* in place; categorising specific cases as editorial is
ongoing work (see prompt 30 in `~/Code/prompts/`).

## Tracked regressions, by section

Counts below are `(case, kind)` failures from
`tests/gfm-spec/snapshot.txt`.

| Section                                  | Spec ref       | Failures |
| ---------------------------------------- | -------------- | -------- |
| Emphasis and strong emphasis             | CM 0.30 §6.2   | 18       |
| List items                               | CM 0.30 §5.2   | 12       |
| HTML blocks                              | CM 0.30 §4.6   | 8        |
| Links                                    | CM 0.30 §6.3   | 6        |
| Link reference definitions               | CM 0.30 §4.7   | 4        |
| Entity and numeric character references  | CM 0.30 §2.5   | 4        |
| Lists                                    | CM 0.30 §5.3   | 4        |
| Tabs                                     | CM 0.30 §2.2   | 3        |
| Thematic breaks                          | CM 0.30 §4.1   | 3        |
| Block quotes                             | CM 0.30 §5.1   | 2        |
| Fenced code blocks                       | CM 0.30 §4.5   | 2        |
| Setext headings                          | CM 0.30 §4.3   | 1        |
| Task list items (extension)              | GFM §5.3       | 1        |
| Backslash escapes                        | CM 0.30 §2.4   | 1        |
| Raw HTML                                 | CM 0.30 §6.8   | 1        |

By comparison kind: 48 AST mismatches, 13 HTML mismatches,
9 idempotence failures.

Most AST mismatches come from pulldown-cmark's text-run chunking
differing from cmark-gfm's; these cases still produce equivalent
output when serialised. The `--mode=verbatim` flag bypasses the
typed-IR path entirely and preserves the source byte-for-byte; users
who need lossless preservation for a specific document should reach
for it rather than expecting the default normalising mode to handle
every edge case.

## How to read the live numbers

```sh
cargo test --release --test gfm_spec gfm_spec_coverage -- --nocapture
```

prints, at the top of its output:

```
gfm spec coverage:
  matching:               <n>
  editorial deviations:   <n>
  tracked regressions:    <n>
```

These are the source of truth; the table above is a snapshot for the
release notes.

## Updating the snapshot

After a deliberate fix (or an accepted editorial deviation):

```sh
# A fix that removes (case, kind) entries from snapshot.txt:
MDWRIGHT_UPDATE_SNAPSHOT=1 cargo test --release --test gfm_spec gfm_spec_snapshot

# An editorial deviation: add a row to tests/gfm-spec/allowlist.toml
# *before* regenerating the snapshot, then run the same command.
```

The snapshot test fails on any drift; CI will not silently accept a
regression that happens to look like an improvement, and an
improvement that isn't reflected in the snapshot fails just as loudly.
