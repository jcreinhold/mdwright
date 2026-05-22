# mdwright spec deviations

The mdwright formatter targets the GFM 0.29-gfm spec (`crates/mdwright/tests/gfm-spec/spec.txt`, vendored from
cmark-gfm). Every example is exercised by `crates/mdwright/tests/gfm_spec.rs` as a `parse → format → parse → format`
round-trip and compared against the source HTML and the normalised event stream.

This document is the user-facing index of where mdwright currently **does not** byte-for-byte round-trip the spec. It is
split into two parts because the underlying mechanism does:

- **Editorial deviations**: choices we have made and intend to keep. Curated in
  `crates/mdwright/tests/gfm-spec/allowlist.toml`. Each entry has a one-line rationale and a pointer to where the
  decision is documented.
- **Tracked regressions**: known divergences that we intend to fix. Recorded in
  `crates/mdwright/tests/gfm-spec/snapshot.txt`. The snapshot is asserted byte-for-byte, so any drift, whether
  regression *or* improvement, fails CI and forces a deliberate update.

The `gfm_spec_coverage` test prints the live count for both groups; the numbers below are a snapshot of the current main
branch.

## Coverage

| Bucket | Examples |
| --- | --- |
| Spec examples total | 672 |
| Matching | 637 |
| Editorial deviations | 35 |
| Tracked regressions | 0 |

A *case* may fail more than one comparison kind (`semantic`, `idempotence`); the snapshot file is keyed by
`(case, kind)` and currently lists no tracked regressions.

## Parser Backend Drift

The formatter round-trip gate is not the same as cmark-gfm renderer equivalence. `cargo xtask parser-audit` compares
mdwright's current pulldown-cmark backend with cmark-gfm and renders mdwright through the opt-in `cmark-gfm` render
profile. The current GFM-spec parser audit has 15 classified HTML differences, 0 source-position differences, and 0
unclassified differences.

The remaining differences are accepted constraints of the current backend:

| Class | Count | Status |
| --- | ---: | --- |
| Emphasis delimiter-stack resolution | 9 | accepted parser-backend drift |
| Raw HTML block indentation/newline spelling | 4 | accepted render drift with stable source facts |
| Task-list examples marked disabled by the spec | 2 | accepted spec-fixture drift |
| Contained upstream parser panic | 1 | converted to `ParseError` |

`[render] profile = "cmark-gfm"` changes only HTML spelling for `mdwright render`: quote escaping, link-destination
escaping, ordinary GFM table layout, task-list checkbox spelling, and one raw-HTML newline case where the parser already
exposes enough structure. It does not change emphasis resolution or parser tree semantics. Full cmark-gfm parser
equivalence would require upstream pulldown-cmark changes, a maintained fork, or a parser backend switch.

## Editorial deviations

### Pulldown text-chunking deviations

35 spec examples currently fail the AST-event comparison only; HTML matches byte-for-byte and round-trip is idempotent.
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

## Tracked regressions

There are currently no tracked GFM-spec formatter regressions. Any future non-allowlisted failure appears in
`crates/mdwright/tests/gfm-spec/snapshot.txt` and fails the snapshot test until it is fixed or deliberately classified.

## mdformat-mkdocs parity deviations

mdwright matches mdformat-mkdocs byte-for-byte for the four Markdown extensions covered in
[Markdown extensions](concepts/extensions.md). The parity test at `crates/mdwright/tests/extension_parity.rs` enforces
this against five committed reference fixtures. Known divergences below; each row exists because the upstream
pulldown-cmark parser doesn't surface enough information for mdwright to round-trip the source faithfully.

| Construct | Source pattern that diverges | Why |
| --- | --- | --- |
| Heading attribute, quoted value | `# H {title="hello world"}` | pulldown-cmark 0.13's heading-attribute parser splits the trailer on whitespace and ignores `"…"` quoting. Pulldown surfaces two attrs (`title="hello`, `world"`) instead of one. mdformat-mkdocs (python-markdown's `attr_list`) handles the quoted form correctly. Tracked upstream; will resolve when pulldown lands the fix. |

The parity test refuses to silently accept new divergences: any byte-for-byte mismatch fails the test and forces a
deliberate add to this table (with a rationale and an upstream pointer) or a fix in mdwright's emit path.

## MyST + Pandoc directive parity

mdwright preserves MyST directive containers, Pandoc fenced divs, inline roles, MyST substitutions, Pandoc inline
attribute spans, and MyST `%` line comments byte-verbatim. See [MyST + Pandoc directives](concepts/myst-pandoc.md) for
the full scope. The bar is **idempotence-on-mode**, not byte-equal round-trip with mdformat-mkdocs: mdformat-mkdocs does
not implement these constructs at all, so there is no upstream reference to diff against. The vendored jupyter-book demo
at `crates/mdwright/tests/external/jupyter_book_minimal/` plus the per-construct regressions at
`crates/mdwright/tests/regressions/{directive_*,inline_role_*,myst_*}.in` are the safety net.

| Construct | Source pattern that diverges | Why |
| --- | --- | --- |
| Malformed `:::{name}` source | Bare `:::{warning} Experimental` with no closer | Pulldown parses the opener as part of a definition-list or paragraph; mdwright's directive overlay matches on byte-range overlap and emits the union of the tree-node range and the directive region, so the bytes survive, but the surrounding misclassified bytes flow through pulldown's normal path. Fix the source by closing the directive. |

## How to read the live numbers

```sh
cargo test --release --test gfm_spec gfm_spec_coverage -- --nocapture
```

prints, at the top of its output:

```
gfm spec coverage:
  total cases:        <n>
  fully matching:     <n>
  intentional dev:    <n>
  tracked regression: <n>
  unexpected:         <n>
```

These are the source of truth; the table above is a snapshot for the release notes.

## Updating the snapshot

After a deliberate fix (or an accepted editorial deviation):

```sh
# A fix that removes (case, kind) entries from snapshot.txt:
MDWRIGHT_UPDATE_SNAPSHOT=1 cargo test --release --test gfm_spec gfm_spec_snapshot

# An editorial deviation: add a row to crates/mdwright/tests/gfm-spec/allowlist.toml
# *before* regenerating the snapshot, then run the same command.
```

The snapshot test fails on any drift; CI will not silently accept a regression that happens to look like an improvement,
and an improvement that isn't reflected in the snapshot fails just as loudly.
