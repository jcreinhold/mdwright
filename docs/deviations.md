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
numbers below are accurate as of the v0.3.0 release.

## Coverage at v0.3.0

| Bucket                 | Examples |
| ---------------------- | -------- |
| Spec examples total    | 672      |
| Matching               | 605      |
| Editorial deviations   | 0        |
| Tracked regressions    | 67       |

A *case* may fail more than one comparison kind (`html`, `ast`,
`idempotence`); the snapshot file is keyed by `(case, kind)` and
currently lists 86 entries across 67 distinct cases.

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

| Section                                  | Failures |
| ---------------------------------------- | -------- |
| Emphasis and strong emphasis             | 18       |
| List items                               | 18       |
| HTML blocks                              | 8        |
| Lists                                    | 6        |
| Links                                    | 6        |
| Setext headings                          | 4        |
| Link reference definitions               | 4        |
| Entity and numeric character references  | 4        |
| Thematic breaks                          | 3        |
| Tabs                                     | 3        |
| Fenced code blocks                       | 3        |
| Block quotes                             | 3        |
| Task list items (extension)              | 2        |
| Raw HTML                                 | 1        |
| Code spans                               | 1        |
| Backslash escapes                        | 1        |
| ATX headings                             | 1        |

By comparison kind: 54 AST mismatches, 19 HTML mismatches,
13 idempotence failures.

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
