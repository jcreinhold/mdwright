# Stability charter

> **Invariant.** Formatting a parsed document preserves Markdown meaning, or refuses the rewrite that would change
> it. Default formatting is identity emit modulo document-boundary normalisation; opt-in style and wrap changes are
> transactional byte rewrites, each verified against document-owned parser facts.

mdwright's correctness rests on three deep modules in `mdwright-document` and `mdwright-format`, not on layered
agreements between consumers:

1. **One pulldown chokepoint** in `mdwright-document`. Every production `pulldown_cmark::Parser` invocation goes through
   private helpers in `crates/mdwright-document/src/parse.rs` that take the private `CanonicalSource<'_>` newtype.
   Construction routes through source canonicalisation, so the type system enforces the chokepoint. Upstream parser
   panics convert to `ParseError` at this boundary.
2. **Structural emit is identity.** `format_document` starts from the parsed document's canonical source bytes; default
   formatting reaches only document-boundary normalisation.
3. **Style canonicalisation and wrapping are verified transactions.** Opt-in rewrites are candidates with owners,
   ranges, ordering, overlap handling, and per-rewrite verification. A failed verification skips that candidate.

The bug class that motivated this design—formatter mutations that perturb their own parse context—survives only as
rewrite candidates. A candidate cannot commit unless the document-level verification predicate accepts it.

## The bug class

As long as any emit site reads source bytes to *choose* its representation, perturbation is possible. The fuzz fixes
catalogued in [`fuzz-history.md`](fuzz-history.md) trace the same shape: a downstream pass *predicted* what pulldown
would do, instead of asking pulldown what it does. Two examples drove the redesign:

- `_*/*_` (5 bytes). Pulldown sees nested emphasis; a predictive formatter emitted `*\*/\**`, which re-parses to a
  single emphasis.
- `**u*~***~`. Pulldown sees one Strong wrapping Emphasis-and-text plus trailing literals; a predictive formatter
  oscillated between `**u*~*\*\*~` and `**u*~~\*\*\*~~` on successive passes.

Removing the read site—preserving source representation byte-for-byte— removes the bug class. Style canonicalisations
that *do* need to choose a representation move into a separate pass where each rewrite verifies locally before
committing.

## The pipeline

```
source → CanonicalSource → pulldown::Parser → typed IR
       → structural emit (per-construct .pretty(), source-preserving)
       → normalize_line_endings_lf
       → [if opts.has_any_canonicalisation(): canonicalise pass]
       → normalize_trailing_newline → apply_end_of_line → out
```

Only document-owned canonicalisation can produce a `CanonicalSource`; only `mdwright-document` invokes `pulldown-cmark`.
Parser panics become `ParseError` at that boundary. The canonicalisation pass iterates internally to a fixed point
(capped at `MAX_CANONICALISE_ITERS = 8`); per-rewrite verification rejects any candidate whose paragraph-window reparse
diverges from the source IR.

## Public API

| Symbol                                                | Behaviour                                                     |
| ----------------------------------------------------- | ------------------------------------------------------------- |
| `Document::parse(&str) -> Result<Document, ParseError>` | Fallible at the parser trust boundary.                      |
| `format_document(&doc, opts) -> String`               | Infallible over an already-parsed document.                   |
| `format_validated(&doc, opts) -> Result<String, FormatError>` | Carries parse failures and semantic divergence.       |
| `semantically_equivalent(a, b) -> Result<bool, ParseError>`   | Reparses both inputs to build semantic signatures.    |

`FmtOptions` style knobs default to `Preserve`. Fluent setters (`with_italic`, `with_strong`, `with_list_marker`,
`with_ordered_list`, `with_thematic_break`, `with_link_def_style`) cover programmatic callers; the TOML keys are
`[fmt] strong`, `[fmt] thematic-break`, and the existing per-knob spellings. User-facing surfaces are documented in
[`docs/src/format/policy.md`](../src/format/policy.md) and [`docs/src/format/style.md`](../src/format/style.md).

## Risk register

| Risk                                                                | Bound                                                                                  | Evidence                                                                                          |
| ------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Canonicalisation's internal convergence loop fails to terminate.    | Capped at `MAX_CANONICALISE_ITERS = 8`; cap exceedance logs `tracing::warn!` and returns the current buffer. | 4096-case property sweep at `tests/properties.rs::canonicalise_document_*_sweep` has never hit the cap. |
| Per-rewrite verification's paragraph window misses cross-paragraph effects. | Rewrites that would affect adjacent paragraphs verify within their own window and skip if the local parse diverges. | Skips are logged; high-skip-rate documents surface in production traces.                          |
| Structural emit edge cases the 4096-case sweep doesn't reach.       | `FmtOptions::default()` regressions tracked in `docs/architecture/round-4-findings/` (empty list item at EOF; ATX trailing-hash). | Both reproduce; both are pre-existing structural-emit bugs surfaced by broader option-space fuzz coverage. |
| Pulldown behaviour drifts between releases.                         | `docs/architecture/pulldown-model.md` documents the invariants; `tests/pulldown_model.rs` fails when pulldown disagrees. | One chokepoint at `src/parse.rs` is the single site any drift mitigation lands.                   |

## Out of scope

- Replacing `pulldown-cmark`. The bug class is about *agreement* with pulldown; a different parser trades one
  disagreement surface for another.
- AST-level structural diff in the verification gate. Event-stream equivalence is sufficient and cheap; AST diff
  amplifies position-noise into false divergence.
- A custom emphasis tokeniser. CM §6.2 is correct; mdwright's job is to produce output that lets pulldown's tokeniser
  reach the same answer as it did on the source.
- Cross-knob canonicalisation modes beyond what `FmtOptions` exposes. For aggressive cross-knob normalisation, use
  mdformat; see the README.

## What the bar is now

Two `rg` invariants guard against regression of the design above:

- `rg 'opts\.(italic|strong|list_marker|thematic|link_def|ordered_list)' src/` returns only the call sites in
  `src/format/canonicalise.rs`. Structural emit does not read style knobs.
- Every `pulldown_cmark::Parser` invocation in `src/` routes through `src/parse.rs::events` or `events_with_offsets`;
  `#[cfg(test)]` exceptions carry an inline justification.

The `normalize_*` post-passes (`normalize_trailing_newline`, `source_has_effective_trailing_newline`,
`normalize_line_endings_lf`, `apply_end_of_line`) all live in `src/format/mod.rs` and are wired in
`src/format/document.rs`. They are boundary-policy transforms, not perturbation sources: `normalize_trailing_newline`
reads source bytes to decide whether the output ends with `\n`; the LF normaliser is a cheap belt-and-braces over the
invariant carried by `Doc::Text` construction.
