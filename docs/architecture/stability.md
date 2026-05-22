# Stability charter

> **Invariant.** Formatting a parsed document preserves Markdown meaning, or refuses the rewrite that would change it.
> Default formatting is identity emit modulo document-boundary normalisation; opt-in style and wrap changes are
> transactional byte rewrites, each verified against document-owned parser facts.

mdwright's correctness rests on three deep modules in `mdwright-document` and `mdwright-format`, not on layered
agreements between consumers:

1. **One pulldown chokepoint** in `mdwright-document`. Every production `pulldown_cmark::Parser` invocation goes through
   private helpers in `crates/mdwright-document/src/parse.rs` that take the private `CanonicalSource<'_>` newtype.
   Construction routes through source canonicalisation, so the type system enforces the chokepoint. Upstream parser
   panics convert to `ParseError` at this boundary.
2. **Structural emit is identity.** `format_document` starts from the parsed document's canonical source bytes; default
   formatting reaches only document-boundary normalisation.
3. **Style canonicalisation and wrapping are rewrite-family operations.** Opt-in rewrites run as ordered families. Each
   family builds a locally non-overlapping normal-form plan, verifies the whole plan, and commits all edits or none.

The bug class that motivated this design—formatter mutations that perturb their own parse context—survives only as
private rewrite-family edits. A family cannot commit unless the document-level verification predicate accepts it.

## The bug class

As long as any emit site reads source bytes to *choose* its representation, perturbation is possible. The bugs that
drove this design all share one shape: a downstream pass *predicted* what pulldown would do, instead of asking pulldown
what it does. Two examples:

- `_*/*_` (5 bytes). Pulldown sees nested emphasis; a predictive formatter emitted `*\*/\**`, which re-parses to a
  single emphasis.
- `**u*~***~`. Pulldown sees one Strong wrapping Emphasis-and-text plus trailing literals; a predictive formatter
  oscillated between `**u*~*\*\*~` and `**u*~~\*\*\*~~` on successive passes.

Removing the read site—preserving source representation byte-for-byte— removes the bug class. Style canonicalisations
that *do* need to choose a representation move into a separate pass where each rewrite family verifies before
committing.

## The pipeline

```
source → CanonicalSource → pulldown::Parser → typed IR
       → structural emit (source-preserving)
       → normalize_line_endings_lf
       → [if opts enables rewrites: rewrite-family pipeline]
       → normalize_trailing_newline → apply_end_of_line → out
```

Only document-owned canonicalisation can produce a `CanonicalSource`; only `mdwright-document` invokes `pulldown-cmark`.
Parser panics become `ParseError` at that boundary. The rewrite-family pipeline reparses after each committed family so
later families see current document facts. Success means a full pass over enabled families commits nothing. If the guard
pass count trips first, mdwright leaves the original source bytes unchanged rather than returning a partially normalized
buffer as success.

## Public API

| Symbol | Behaviour |
| --- | --- |
| `Document::parse(&str) -> Result<Document, ParseError>` | Fallible at the parser trust boundary. |
| `format_document(&doc, opts) -> String` | Infallible over an already-parsed document. |
| `format_validated(&doc, opts) -> Result<String, FormatError>` | Carries parse failures and semantic divergence. |
| `semantically_equivalent(a, b) -> Result<bool, ParseError>` | Reparses both inputs to build semantic signatures. |

`FmtOptions` style knobs default to `Preserve` except GFM tables, which default to compact normal form. Fluent setters
(`with_italic`, `with_strong`, `with_list_marker`, `with_ordered_list`, `with_thematic_break`, `with_table`,
`with_link_def_style`) cover programmatic callers; the TOML keys are `[fmt] strong`, `[fmt] thematic-break`, and the
existing per-knob spellings. User-facing surfaces are documented in
[`docs/src/format/policy.md`](../src/format/policy.md) and [`docs/src/format/style.md`](../src/format/style.md).

## Risk register

| Risk | Bound | Evidence |
| --- | --- | --- |
| A rewrite family contains overlapping local edits. | The family plan rejects before verification; no individual edit is selected out of the overlap. | Unit tests in `mdwright-format` cover local-overlap rejection. |
| The rewrite-family pipeline never reaches a no-commit pass. | The guard pass count logs `tracing::warn!` and returns the original source bytes unchanged. | Idempotence regressions and fuzz replay cover known sustained-fuzz failures. |
| Verification misses a cross-paragraph effect. | Families verify the whole document and skip if the document or math signature diverges. | Skips are logged; high-skip-rate documents surface in production traces. |
| Structural emit edge cases the 4096-case sweep doesn't reach. | Two accepted `FmtOptions::default()` regressions: an empty list item at EOF, and an ATX heading with a trailing hash. | Both reproduce as pre-existing structural-emit bugs surfaced by broader option-space fuzz coverage. |
| Pulldown behaviour drifts between releases. | `docs/architecture/pulldown-model.md` documents the invariants; `tests/pulldown_model.rs` fails when pulldown disagrees. | One chokepoint at `crates/mdwright-document/src/parse.rs` is the single site any drift mitigation lands. |

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

- `rg 'opts\.(italic|strong|list_marker|thematic|link_def|ordered_list)' crates/mdwright-format/src/` returns only the
  style-policy call sites in `crates/mdwright-format/src/format/canonicalise.rs`. Structural emit does not read style
  knobs.
- Every production `pulldown_cmark::Parser` invocation routes through the document parse boundary; `#[cfg(test)]`
  exceptions carry an inline justification.

The `normalize_*` post-passes (`normalize_trailing_newline`, `source_has_effective_trailing_newline`,
`normalize_line_endings_lf`, `apply_end_of_line`) live in `crates/mdwright-format/src/format/mod.rs` and are wired
through the public formatting entry points. They are boundary-policy transforms, not perturbation sources:
`normalize_trailing_newline` reads source bytes to decide whether the output ends with `\n`; the LF normaliser checks
the invariant carried by document construction.
