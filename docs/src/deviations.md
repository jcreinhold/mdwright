# mdwright spec deviations

The mdwright formatter targets the GFM 0.29-gfm spec (`tests/gfm-spec/spec.txt`, vendored from cmark-gfm). Every example
is exercised by `tests/gfm_spec.rs` as a `parse → format → parse → format` round-trip and compared against the source
HTML and the normalised event stream.

This document is the user-facing index of where mdwright currently **does not** byte-for-byte round-trip the spec. It is
split into two parts because the underlying mechanism does:

- **Editorial deviations** — choices we have made and intend to keep. Curated in `tests/gfm-spec/allowlist.toml`. Each
  entry has a one-line rationale and a pointer to where the decision is documented.
- **Tracked regressions** — known divergences that we intend to fix. Recorded in `tests/gfm-spec/snapshot.txt`. The
  snapshot is asserted byte-for-byte, so any drift — regression *or* improvement — fails CI and forces a deliberate
  update.

The `gfm_spec_coverage` test prints the live count for both groups; the numbers below are a snapshot of the current main
branch.

## Coverage

| Bucket               | Examples |
| -------------------- | -------- |
| Spec examples total  | 672      |
| Matching             | 615      |
| Editorial deviations | 35       |
| Tracked regressions  | 22       |

A *case* may fail more than one comparison kind (`html`, `ast`, `idempotence`); the snapshot file is keyed by
`(case, kind)` and currently lists 35 entries across 22 distinct cases.

## Editorial deviations

### Pulldown text-chunking deviations

35 spec examples currently fail the AST-event comparison only — HTML matches byte-for-byte and round-trip is idempotent.
The mismatch reflects pulldown-cmark's text-run chunking: pulldown splits long runs of text into events at points
cmark-gfm does not, so the normalised `Event::Text(…)` stream differs even though every other event lines up and every
rendered HTML byte agrees.

The triage rule, applied at the snapshot level, is:

```
For each (case, kinds) in snapshot.txt:
  if kinds == {"ast"} and case has no other entry:
    -> allowlist.toml (bucket = "pulldown-text-chunking")
  else:
    -> stays in snapshot.txt (tracked regression)
```

Affected cases: 5, 6, 7 (Tabs, CM §2.2); 16, 19 (Thematic breaks, CM §4.1); 61 (Setext headings, CM §4.3); 102, 103
(Fenced code blocks, CM §4.5); 214, 230 (Block quotes, CM §5.1); 232, 242, 248, 249, 251, 252, 256, 264, 265, 266, 268
(List items, CM §5.2); 320 (Backslash escapes, CM §2.4); 321, 324, 330, 333 (Entity refs, CM §2.5); 393, 411 (Emphasis,
CM §6.2); 499, 500, 503, 520, 528, 536 (Links, CM §6.3); 640 (Raw HTML, CM §6.8).

The bucket name is load-bearing: if a future per-case investigation disproves the chunking explanation for one of the
cases above, remove its entry from `allowlist.toml` and let it re-enter the snapshot as a tracked regression.

## Tracked regressions, by section

Counts below are `(case, kind)` failures from `tests/gfm-spec/snapshot.txt`.

| Section                      | Spec ref     | Failures |
| ---------------------------- | ------------ | -------- |
| Emphasis and strong emphasis | CM 0.30 §6.2 | 16       |
| HTML blocks                  | CM 0.30 §4.6 | 8        |
| Link reference definitions   | CM 0.30 §4.7 | 4        |
| Lists                        | CM 0.30 §5.3 | 4        |
| Thematic breaks              | CM 0.30 §4.1 | 1        |
| List items                   | CM 0.30 §5.2 | 1        |
| Task list items (extension)  | GFM §5.3     | 1        |

By comparison kind: 13 HTML mismatches, 13 AST mismatches, 9 idempotence failures.

All entries here are real divergences in mdwright's output, not pulldown chunking artefacts — those moved to the
editorial allowlist (see *Pulldown text-chunking deviations* above). Each case will need its own root-cause analysis to
fix; some overlap with parked fuzz finds under `fuzz/known-issues/`. The `--mode=verbatim` flag bypasses the typed-IR
path entirely and preserves the source byte-for-byte; users who need lossless preservation for a specific document
should reach for it rather than expecting the default normalising mode to handle every edge case.

## mdformat-mkdocs parity deviations

mdwright matches mdformat-mkdocs byte-for-byte for the four Markdown extensions covered in
[Markdown extensions](concepts/extensions.md). The parity test at `tests/extension_parity.rs` enforces this against
five committed reference fixtures. Known divergences below; each row exists because the upstream pulldown-cmark
parser doesn't surface enough information for mdwright to round-trip the source faithfully.

| Construct                       | Source pattern that diverges      | Why                                                                                                                                                              |
| ------------------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Heading attribute, quoted value | `# H {title="hello world"}`       | pulldown-cmark 0.13's heading-attribute parser splits the trailer on whitespace and ignores `"…"` quoting. Pulldown surfaces two attrs (`title="hello`, `world"`) instead of one. mdformat-mkdocs (python-markdown's `attr_list`) handles the quoted form correctly. Tracked upstream; will resolve when pulldown lands the fix. |

The parity test refuses to silently accept new divergences: any byte-for-byte mismatch fails the test and forces a
deliberate add to this table (with a rationale and an upstream pointer) or a fix in mdwright's emit path.

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

These are the source of truth; the table above is a snapshot for the release notes.

## Updating the snapshot

After a deliberate fix (or an accepted editorial deviation):

```sh
# A fix that removes (case, kind) entries from snapshot.txt:
MDWRIGHT_UPDATE_SNAPSHOT=1 cargo test --release --test gfm_spec gfm_spec_snapshot

# An editorial deviation: add a row to tests/gfm-spec/allowlist.toml
# *before* regenerating the snapshot, then run the same command.
```

The snapshot test fails on any drift; CI will not silently accept a regression that happens to look like an improvement,
and an improvement that isn't reflected in the snapshot fails just as loudly.
