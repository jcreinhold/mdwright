# Round-4 fuzz findings

Pre-existing structural-emit edge cases surfaced when the fuzz harness's `opts_from_byte` was widened to cover
canonicalisation modes (see `fuzz/fuzz_targets/{fuzz_parse_format,fuzz_idempotence}.rs`). Neither is a canonicalisation
bug: canonicalisation surfaces the cases by changing the bytes structural emit operates on next pass, tripping
byte-equality where the source-byte shape happened to round-trip trivially before.

The canonicalisation pass is already maximally defensive: each rewrite verifies its parse window; the pass iterates
internally to a fixed point. The non-idempotence is inherited from the structural layer.

## `01-empty-list-item-marker-cascade.in`

Input: `` `**L0B\n*\t\tL0B*\t\n+ ``. Default format produces semantic divergence (collapses a `*` and a backtick into
different IR). With `list_marker = "asterisk"` the trailing `+ ` rewrites to `* `, and structural emit on the rewritten
buffer collapses the trailing newline run differently than on the `+ ` version.

Two underlying structural-emit asymmetries:

1. Empty list items at end-of-document are emitted differently by marker character.
2. Backtick text in paragraphs adjacent to fenced code blocks produces non-byte-equal output between passes.

## `02-heading-trailing-hash.in`

Input: `# # #` (5 bytes). Default format produces `# #` (3 bytes), dropping a `#` even though pulldown's ATX
trailing-hash rule treats the trailing `#` as decoration on the heading body `#`. The heading emitter strips a trailing
hash without preserving the original count.

Superseded by the type-level refactor analysed at [`../round-5-findings/`](../round-5-findings/) (cluster A).

The `gfm_spec_coverage` snapshot is clean, and the property-test matrix (15 modes × {idempotence, semantic-equivalence}
at 4096 cases) is green; these two cases are tracked here as the acceptance suite for the next pass at structural-emit
edge cases.
