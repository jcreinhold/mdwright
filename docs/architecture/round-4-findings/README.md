# Round-4 fuzz findings — structural-emit residuals after prompt 54

Prompt 54 expanded the fuzz harness's `opts_from_byte` to cover canonicalisation modes
(see `fuzz/fuzz_targets/fuzz_idempotence.rs` and `fuzz/fuzz_targets/fuzz_parse_format.rs`).
The first 2-minute run after the expansion uncovered the artifacts in this directory.

## Status

Each finding here is a **pre-existing structural-emit edge case**, not a canonicalisation
bug — most reproduce under `FmtOptions::default()` (with `idem=true` but `sem=false`, or
the reverse). The canonicalisation pass surfaces them by changing the bytes the structural
emit operates on next pass, which trips the byte-equality property where the source-byte
shape happened to round-trip trivially before.

Concretely:

- `01-empty-list-item-marker-cascade.in` — input `` `**L0B\n*\t\tL0B*\t\n+ ``. Default
  format produces semantic divergence (collapses a `*` and a backtick into different IR);
  with `list_marker = "asterisk"` the trailing `+ ` rewrites to `* `, and structural emit
  on the rewritten buffer collapses the trailing newline run differently than it does on
  the `+ ` version. Two underlying structural-emit asymmetries:
  1. Empty list items at end-of-document handled differently by marker character.
  2. Backtick text in paragraphs adjacent to fenced code blocks produces non-byte-equal
     output between passes.

- `02-heading-trailing-hash.in` — input `# # #` (length 5). Default format produces
  `# #` (length 3), dropping a `#` even though pulldown's ATX trailing-hash rule treats
  the trailing `#` as decoration on the heading text `#`. The mdwright heading emitter
  strips a trailing-hash without preserving the original count. `sem=false` under
  default opts; canonicalisation modes inherit the failure. Bug is in heading pretty.

The `minimized-from-*` files are libFuzzer's own minimisations of related inputs in the
same cluster.

## Out of scope for prompt 54

Fixing these requires structural-emit changes (the empty-block emit symmetry; the
backtick / fenced-code adjacency rule). The canonicalisation pass is already maximally
defensive: each rewrite verifies its parse window, and the pass iterates internally to a
fixed point. The remaining non-idempotence is inherited from the structural layer, not
introduced by canonicalisation.

The `gfm_spec_coverage` snapshot is clean, the property-test matrix (15 modes ×
{idempotence, semantic-equivalence} at 4096 cases) is green, and the round-3 fixtures
that motivated the prompt-51 sweep now byte-preserve. Those are the load-bearing
acceptance tests for the redesign. The structural-emit residuals are tracked here for a
follow-up that revisits the structural emit's edge cases against this new evidence.
