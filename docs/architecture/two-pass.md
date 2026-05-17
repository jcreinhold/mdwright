# The iterative-draft formatter

> **Invariant.** Every output of `Document::format` is a fixed point of
> `render(source_tree, FlankSource::Draft(this_output))`. Convergence is
> bounded; non-convergent inputs fall back to verbatim source emission.

mdwright's formatter must decide how to emit each emphasis, strong, link, and image site so the result re-parses to
the source's IR. Many of those decisions depend on the bytes *adjacent* to the site — `CommonMark` §6.2's flanking
rules look at the single character on each side of a delimiter run. The local emitter does not see those bytes:
they come from sibling emitters that run earlier or later in the walk.

Before this design, the inline walker tried to **predict** the flank by threading
`ambient_left` / `ambient_right` strings down every wrapper recursion, synthesising what the next-sibling output
would be. The prediction was a stack of best-guess approximations; each one was correct in isolation, but the stack
broke wherever the actual emit diverged from the prediction. Three round-2 patches and two round-3 findings landed
against this scheme without ever exhausting the bug class.

The replacement is to stop predicting. The formatter renders the source IR repeatedly; on each iteration the safety
ladder reads flank bytes from the **previously-rendered draft** instead of from a synthesised prediction. The first
iteration uses *source itself* as its initial draft (with an identity source-to-draft correspondence), so flank
decisions start from real bytes around real nodes. Subsequent iterations swap the draft for the previous output and
re-render. The loop returns on the first pair of consecutive equal outputs — a fixed point of "render the source IR
with draft-derived flank."

## The four invariants

1. **One mechanism.** Every render uses `FlankSource::Draft(view)`. There is no special "isolated" pass. The
   distinction between the first and subsequent iterations is purely *which draft is supplied*, not how it is
   consulted. Adding a future emit decision that needs draft-derived input (setext heading body, list marker style,
   …) requires extending `FlankSource::flank_for`; it does not require introducing a new pass concept.
2. **Source as the initial draft.** Iteration 0 passes `DraftView { bytes: source, tree: source_tree,
   source_to_draft: identity }`. Source bytes are the best flank approximation available before any IR-driven emit
   has run, and the source tree gives node positions for free. This makes the iteration uniform — there is no
   "cold start" that uses a different decision rule.
3. **Convergence is byte equality.** If iteration N produces the same bytes as iteration N-1, the output is a
   fixed point of the iteration map. The comparison is on the structural payload, not on trailing-newline or
   end-of-line policy bytes; those run *after* convergence so they cannot count as a non-convergent transition.
4. **Bounded loop with a typed error.** `MAX_PASSES = 2` (one rectifying pass + one confirming pass). If pass 3
   would be needed, the safety ladder is in a flank-decision cycle — that is a real design flaw to surface, not a
   transient to absorb with more iterations. `ConvergenceError::DidNotConverge` is the error; `Document::format`
   falls back to verbatim source emission with a `tracing::warn!`, `Document::format_validated` returns
   `FormatError::DidNotConverge { source, last_draft }` so the caller can decide.

## Why `MAX_PASSES = 2` is the principled bound

The iteration produces a sequence D₀, D₁, D₂, … where D₀ uses source as the draft and Dₙ uses Dₙ₋₁ as the draft.
Convergence is "Dₙ == Dₙ₋₁". `MAX_PASSES = 2` allows the loop to check D₁ == D₀ and (if not) D₂ == D₁. Two
checks suffice for the meaningful cases:

- D₁ == D₀ means the source's own bytes give flank decisions that re-emit to those same bytes — most documents.
- D₂ == D₁ means iteration 1 rectified some decision, and iteration 2 confirmed the rectified output is stable.

Needing D₃ != D₂ to converge would mean iteration 2's rectification destabilised some *other* decision whose flank
moved as a side effect — a cycle in the flank-decision dependency graph. mdwright's emit decisions are local
(per emphasis / strong / link / image site, depending on immediate neighbours), so cycles arise only when two
adjacent sites' decisions mutually depend on each other's emitted bytes. That is the pathology the verbatim
fallback exists to cover; absorbing it with extra iterations would mask the cycle, not resolve it.

## Pass boundary vs the post-pass chain

The convergence loop owns the structural render. The post-pass chain (`normalize_trailing_newline`,
`apply_end_of_line`) runs *once*, after convergence:

```
render(FlankSource::Draft(source))  ──► D₀
       normalize_line_endings_lf
                │
                ├─► render(FlankSource::Draft(D₀))  ──► D₁ ──┐
                │           normalize_line_endings_lf         │
                │                                              ├─ D₁ == D₀ ? return apply_tail(D₀)
                │                                              ▼
                │           render(FlankSource::Draft(D₁))  ──► D₂ ──┐
                │                   normalize_line_endings_lf         │
                │                                                      ├─ D₂ == D₁ ? return apply_tail(D₁)
                │                                                      ▼
                │                                              return Err(DidNotConverge { D₂ })
                ▼
   `normalize_line_endings_lf` runs inside the loop so the next iteration's `CanonicalSource::from_source`
   accepts the draft (CR/NUL canonicalisation is idempotent on a CR-free buffer).
```

This boundary is forward-compatible with prompt 48, which deletes `normalize_trailing_newline` by folding the
decision into each block emitter. After prompt 48 the only surviving tail policy is `apply_end_of_line` — and that
is a configured output transform, not a defensive normalisation.

## What this design does *not* address

Two-pass solves emit decisions whose inputs are *flank bytes*. It does not solve emit decisions whose correctness
depends on the **full structural shape** of the output's re-parse. Specifically:

- **Nested-emphasis preservation.** Pulldown's emphasis pairing for `_*/*_` produces nested Emphasis(Emphasis("/")).
  mdwright's safety ladder verifies only that the *outer* wrap re-parses as a single Emphasis run; it does not
  verify that the inner Emphasis survives. With the source-style normaliser preferring `*` over `_`, the outer
  delimiter gets rewritten, the inner `*` collides with the new outer `*`, and the escape ladder produces
  `*\*/\**` (text only). The output is not semantically equivalent to the source.
- **Multi-construct interaction.** Strong containing Emphasis adjacent to Strikethrough involves emit decisions
  whose draft flank is correct individually but whose interaction across construct boundaries can re-pair on
  re-parse. The round-3 finding `02-idempotence-emphasis-strong-strikethrough.in` is in this family.

Both are tracked in `docs/architecture/round-3-findings/`. The fix is a separate prompt (extend the safety ladder
to verify the full nested IR shape, not just outer-wrap survival); the two-pass mechanism is necessary but not
sufficient for that work.

## Why we rejected typed `DraftOutput` / `ConvergedOutput` wrappers

The stability charter's original sketch (`docs/architecture/stability.md` "Type sketch") proposed two newtypes —
`DraftOutput(String)` and `ConvergedOutput(String)` — to encode the pipeline state as types. We rejected the
sketch:

- Both are pass-through wrappers around `String`. Internal representation == public interface == one field. Per
  Ousterhout ch 7.4, the asymmetry that creates depth is missing.
- Each has exactly one production site and one consumption site within `format_document`. The typestate prevents
  misuse at a single call site; the discipline is local and easily enforced by reading the function.
- Classitis: if a small type cannot be described in one sentence beyond "marker for state X," fold it into its
  caller.

The volatile decision worth encoding in a type is "where does flank come from?" That lives in `FlankSource`. The
pipeline state (which iteration we are in) lives in `format_document`'s local variables, which is the right home
for it.

## Cost

Per format: one render plus up to two re-renders, each preceded by a `Source::new` + `Ir::parse` to build the
next draft tree. For a typical document with no flank-decision rectification, iteration 1 produces D₁ == D₀ on the
first check and the loop returns after one re-render. For documents whose source-flank decisions differ from
draft-flank decisions (round-2 oscillation seed and similar), iteration 2 confirms the rectified output and the
loop returns. The corpus benches measured ±3 % vs the pre-refactor baseline (recorded in
`benches/format_bench.rs`); the cost is dominated by `Ir::parse` of the draft, not the render itself.

If pass-2 cost shows up in a future bench: a per-paragraph "needs pass 2" gate (paragraph contains emphasis /
strong / link / image children) would let the second-iteration emit splice unchanged paragraph bytes from the
draft and only re-render paragraphs with flank-sensitive sites. That is a local optimisation, not a redesign.

## Related architecture documents

- `docs/architecture/stability.md` — the charter that names the bug class and the four-move sweep
  (prompts 46–49).
- `docs/architecture/pulldown-model.md` — the per-construct invariants the safety ladder relies on. Every flank
  decision the formatter makes depends on the rules documented there; this design relies on the rules being
  drift-tested by `tests/pulldown_model.rs`.
- `docs/architecture/round-3-findings/README.md` — the inputs that motivate the next move
  (nested-IR-shape safety verification).
