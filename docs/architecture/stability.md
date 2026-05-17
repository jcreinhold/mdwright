# Stability charter

> **Invariant.** Every output of `Document::format` is a fixed point of `format` and is semantically equivalent to
> its source. The runtime gate enforces both.

> **Sweep status (post-deletion).** Prompt 51 made structural emit pure preservation: every `.pretty()` reads source
> bytes and never consults `FmtOptions`. With perturbation gone, the iterative-draft loop introduced in prompt 47
> was a trivial fixed point on its first iteration. A follow-up deletion (this sweep) removed `src/format/emit_safety.rs`,
> `FlankSource` / `DraftView` / `FlankCtx`, `Tree::corresponding_node_map`, the `ConvergenceError` /
> `FormatError::DidNotConverge` pair, the `MAX_PASSES` loop, and the `verbatim_source_fallback` path —
> single-pass `format_document` returns `String` directly. The per-construct safety ladder no longer exists; future
> style canonicalisation (asterisk-only, underscore-only, marker normalisation) lands as a separate verified
> byte-rewrite pass over the structural output. The Type sketch and Risk register sections below describe state as
> of prompt 47 and are kept for historical reference only.

mdwright's correctness today rests on a circle of agreements between the IR builder, the formatter's per-construct
emitters, the safety ladder in `format::emit_safety`, and the runtime semantic gate. Each agreement is correct in
isolation, but the circle is held together by **every consumer re-deriving pulldown's behaviour from source bytes**.
Bugs of this shape repopulate as fast as they are fixed: prompt 32 reached fuzz-zero, prompt 44 reached it again,
and a round-3 verification immediately produced two more findings in the same family
(`docs/architecture/round-3-findings/`).

This charter specifies the architectural moves that make the bug class unrepresentable. It is the gate for prompts
46-49; each subsequent prompt's success is measured against the invariants this document encodes.

## The bug class

Three round-2 fixes after prompt 44 — `36ded18` (`oracle-domain`), `223cd28` (`boundary-newline-policy`),
`0b5eaf7` (`emphasis-flank-oscillation`) — are local patches to instances of the same shape: a downstream pass
*predicts* what pulldown would do, instead of asking pulldown what it does. Each fix is correct; none address the
shape.

The round-3 findings confirm the shape persists. `_*/*_` (5 bytes) — pulldown sees nested emphasis; mdwright emits
`*\*/\**` (one outer emphasis, escaped body), which re-parses to a single emphasis and fails the gate.
`**u*~***~` — pulldown sees one Strong wrapping Emphasis-and-text plus trailing literals; mdwright produces
`**u*~*\*\*~` on pass 1 and `**u*~~\*\*\*~~` on pass 2. Both findings landed *after* the round-2 ambient-threading
workaround. See [`fuzz-history.md`](fuzz-history.md): 9 of the last 22 fuzz fixes belong to one pattern (output
decision consults source bytes); 5 more belong to "no single chokepoint for pulldown invocation." Per-finding
patches are a treadmill.

The underlying cause is structural. Three properties together would make the bug class impossible:

1. **One chokepoint** through which every `pulldown_cmark::Parser` invocation passes — so canonicalisation policy
   lives in one place and every consumer sees the same bytes.
2. **Output-derived decisions** — emit choices read the bytes the formatter has already produced, not the source
   bytes whose interpretation the formatter is in the middle of rewriting.
3. **Fixed-point gate** — the runtime contract that `format(format(s)) == format(s)`, not just `equivalent(s,
   format(s))`. Non-idempotent emits become an error at the source, not a CI signal.

## The four moves

### Prompt 46 — Canonical-source chokepoint + pulldown-quirks model [LANDED]

Every `pulldown_cmark::Parser` invocation in `src/` now goes through `src/parse.rs::events` (or
`events_with_offsets`), both of which take a `CanonicalSource<'_>` (`src/source.rs`). The newtype's only public
constructor (`CanonicalSource::from_source`) routes through `Source::canonicalise`, so the type system enforces the
chokepoint discipline. Verified: `rg 'Parser::new_ext|Parser::new\(' src/` returns exactly two hits, both in
`src/parse.rs`. Pattern #1 eliminated.

`docs/architecture/pulldown-model.md` documents the per-construct invariants the formatter relies on (line endings,
trailing blank lines, emphasis pairing scope, ref-label normalisation, HTML block boundaries, emphasis-event range
semantics, strong vs nested emphasis). Drift-tested by `tests/pulldown_model.rs`: one test per rule, each failing
with a message that names the doc section to update *before* changing mdwright code.

Side benefits: the per-event CR scrub in `format::semantic::canonical_events` is gone (input is provably CR-free);
the per-site `Options::empty() + insert()` boilerplate collapses to one `parse::FORMATTER_OPTIONS` constant; the
options drift between the safety ladder's strikethrough-only set and the full formatter set is fixed. `render_html`
now also canonicalises (CR→LF, NUL→U+FFFD), matching `Document::parse`.

### Prompt 47 — Output-derived emit (iterative-draft) [LANDED]

The ambient-string workaround (`extend_ambient` / `prepend_close` / `concat_ambient` / `rendered_so_far` in
`src/format/inline.rs`) is gone. In its place: every render uses `FlankSource::Draft(view)` where `view` is a
`DraftView { bytes, tree, source_to_draft }`. The first iteration uses source as the initial draft (identity
correspondence map); subsequent iterations use the previous iteration's output as the next draft. The convergence
loop returns on the first pair of consecutive equal outputs.

`MAX_PASSES = 2` (one rectifying iteration, one confirming iteration). Failure raises
`ConvergenceError::DidNotConverge`; `Document::format` falls back to verbatim source emission with a
`tracing::warn!`, `Document::format_validated` propagates a new `FormatError::DidNotConverge { source, last_draft }`
variant. See `docs/architecture/two-pass.md` for the full design, including why typed `DraftOutput` /
`ConvergedOutput` wrappers were rejected (single-producer / single-consumer classitis around `String`).

Decision-read sites converted: emphasis / strong flank in `src/format/inline.rs` (via
`ctx.flank.flank_for(node_id)`), link / image body identity in `src/cm/inline/link.rs` (via `body_text_for_decision`
reading draft body bytes when available), and paragraph verbatim eligibility in `src/cm/block/paragraph.rs` (the
predicate is now "the IR-driven pass-1 emit byte-matches source").

**Round-3 findings — partially addressed.** Both round-3 inputs exercise nested-IR-shape preservation, not just
flank-derived emit. The safety ladder verifies that the *outer* emphasis / strong run re-parses correctly; it does
not verify that an emit decision (e.g. delimiter renormalisation of the outer wrap) preserves the *inner*
structure. `_*/*_` → `*\*/\**` is the canonical case: the two-pass mechanism produced the bytes pass-1's emit
chose, the convergence loop confirmed those bytes as a fixed point of "render with draft flank," but the rendered
bytes do not re-parse to the source's nested-emphasis IR. The fix belongs to a follow-up prompt that extends
`format::emit_safety::parses_as_single_run` to verify the full nested-IR shape, not just the outer wrap.
Fixtures remain at `docs/architecture/round-3-findings/`; they have not been promoted to `tests/regressions/`
pending that work.

### Prompt 48 — Structural emission (delete `normalize_trailing_newline`)

Move the trailing-newline shape decision into the block-level emitter. The last block in the document already
knows whether it ended on `\n` (indented code block content, fenced code block content, paragraph with hard-break,
table row). Folding the decision into each block kind's `pretty()` eliminates the post-pass that has been guessing
from source bytes via `source.trim_end_matches([' ', '\t']).ends_with('\n')`
(`src/format/mod.rs:44-63`).

Keep `apply_end_of_line` (legitimate output-format policy). Reduce `normalize_line_endings_lf` to a `debug_assert!`
that asserts CR-cleanliness — if `Doc::Text`'s construction-time normalisation is comprehensive (it is, per
`b4e34dd`), the defensive runtime pass is dead weight in release.

Eliminates pattern #3. Deletes `normalize_trailing_newline`, `source_has_effective_trailing_newline`, and the
defensive `normalize_line_endings_lf` calls duplicated in `src/format/semantic.rs:256,407,409,423,425`.

### Prompt 49 — Fixed-point gate + architectural proptests + redundant-ladder deletion

Strengthen `Document::format_validated` to enforce idempotence:

```rust
fn format_validated(&self, opts: &FmtOptions) -> Result<String, FormatError> {
    let pass1 = self.format(opts);
    let pass2 = Document::parse(&pass1).format(opts);
    if pass1 != pass2 {
        return Err(FormatError::NotIdempotent { pass1, pass2 });
    }
    // existing semantic check...
}
```

Add architectural proptests in `tests/properties.rs`: for every well-formed input the generator produces, assert
`format_validated` returns `Ok` (i.e. both equivalence *and* idempotence). The fuzz-zero re-verification then has
two contracts to hold, not one.

Confirm-or-delete `emit_emphasis_safely`'s tiers 3-4 (`src/format/emit_safety.rs:298-316`). With pass-2's
output-derived flank from prompt 47, the flip-delimiter and flip-plus-escape branches should be unreachable;
instrumentation in CI for one release confirms before deletion. Promote the round-3 findings to
`tests/regressions/fuzz_round3_*.in`.

Eliminates patterns #4 and #6.

## Type sketch

The volatile decision worth encoding in a type is **where flank bytes come from**. That lives in `FlankSource`,
not in pipeline-state wrappers:

```rust
// src/source.rs (prompt 46, landed)
pub(crate) struct CanonicalSource<'a> { bytes: &'a str }
// The sole production caller of pulldown_cmark::Parser:
pub(crate) fn events(src: CanonicalSource<'_>, opts: Options) -> Parser<'_> { … }

// src/format/emit_safety.rs (prompt 47, landed)
pub(crate) enum FlankSource<'a> {
    Isolated,                      // exists for the safety-ladder unit tests; not used in production
    Draft(&'a DraftView<'a>),
}
pub(crate) struct DraftView<'a> {
    pub bytes: &'a str,
    pub tree: &'a Tree,
    pub source_to_draft: &'a [Option<NodeId>],
}
impl<'a> FlankSource<'a> {
    pub(crate) fn flank_for(self, source_id: NodeId) -> FlankCtx<'a> { … }
}

// src/format/document.rs (prompt 47, landed)
pub(crate) fn format_document<'a>(…) -> Result<String, ConvergenceError> {
    // Initial render uses source itself as the draft (identity correspondence).
    // Subsequent iterations use the previous iteration's output. Loop returns
    // when two consecutive iterations produce equal bytes.
}

// src/document.rs (prompt 47, landed)
impl Document {
    pub fn format(&self, opts: &FmtOptions) -> String {
        match format::format_document(…) {
            Ok(s) => s,
            Err(ConvergenceError::DidNotConverge { .. }) => verbatim_source_fallback(…),
        }
    }
    pub fn format_validated(&self, opts: &FmtOptions) -> Result<String, FormatError> {
        let formatted = format::format_document(…)
            .map_err(|ConvergenceError::DidNotConverge { last_draft }|
                FormatError::DidNotConverge { source: …, last_draft })?;
        // existing semantic-equivalence check follows
    }
}
```

`CanonicalSource`, `FlankSource`, and `DraftView` are `pub(crate)`. The public API
(`Document::parse(&str)`, `Document::format(&FmtOptions)`, `Document::format_validated`,
`mdwright::semantically_equivalent`) is unchanged except for the additive `FormatError::DidNotConverge` variant.

The rejected alternative (`DraftOutput(String)` + `ConvergedOutput(String)` typed wrappers) is documented in
`docs/architecture/two-pass.md`: both are pass-through wrappers around `String` with single producer and single
consumer each — classitis with no information hidden behind the boundary.

## Public API contract (no breakage)

- `Document::parse(source: &str) -> Document` — unchanged. Still infallible.
- `Document::format(&self, opts: &FmtOptions) -> String` — unchanged signature; the body now runs the two-pass +
  convergence under the hood. Best-effort fallback on convergence failure preserves the infallible signature.
- `Document::format_validated(&self, opts: &FmtOptions) -> Result<String, FormatError>` — `FormatError` gains a
  `NotIdempotent { pass1, pass2 }` variant alongside the existing `SemanticDivergence`. Callers matching the enum
  exhaustively get a compiler nudge; callers using the `Display` impl see a clear message.
- `mdwright::semantically_equivalent(a: &str, b: &str) -> bool` — unchanged.

The mdBook docs at `docs/src/` describe the public API; the four moves do not touch those pages except the
`changelog.md` entry per move.

## Architecture diagram

```
raw &str
    │
    ▼
Source::new ─── canonicalise ───► CanonicalSource(&str) ─┐
                                                         │
                                                         ▼
                                          pulldown::Parser  (sole call site)
                                                         │
                                                         ▼
                                                  typed IR  (Tree, math overlay, refs)
                                                         │
                                                         ▼
                                          format_draft (pass 1: structural, IR-driven)
                                                         │
                                                         ▼
                                             DraftOutput(String) + deferred markers
                                                         │
                                                         ▼
                                          converge (pass 2: re-emit deferred sites
                                                    reading draft bytes for flank)
                                                         │
                                                         ▼
                                           ConvergedOutput(String)
                                                         │
                                                         ▼
                                      fixed-point check: format(out) == out
                                       + equivalence check: equivalent(in, out)
                                                         │
                                                         ▼
                                                       out
```

Every arrow is enforced by the type of its source: only `Source::as_canonical()` produces a `CanonicalSource`; only
`format_draft` produces a `DraftOutput`; only `converge` produces a `ConvergedOutput`. There is no public
constructor for any of them.

## Risk register

| Risk | Mitigation |
|---|---|
| Two-pass emit is ≈ 2× wall-clock on documents with many forward-look-dependent inline sites (emphasis-heavy paragraphs). | Pass 2 visits only the deferred markers pass 1 left, not the whole document. Most emit sites have IR-determined output (paragraphs, lists, code, tables) and are pass-1-only. The `format/medium` and `format/corpus` Criterion benches must stay within ±5 % per move. |
| Refactor blast radius for prompt 46: the chokepoint touches every `Parser::new_ext` caller plus the gate plus the safety ladder. | Land the chokepoint behind `#[deny(clippy::disallowed_methods)]` configured to disallow `Parser::new_ext` outside `src/source.rs` (and the `#[cfg(test)]` helpers). Any new caller fails to compile. |
| Deleting `normalize_trailing_newline` in prompt 48 routes the decision through every block emitter; a missed case strips or adds a `\n` somewhere. | Before deletion, prompt 48 adds a structural proptest (`tests/properties.rs`) that for every block kind, the formatter's trailing-byte shape matches the IR's `is_last_block_open_terminated`. The proptest runs before the deletion commit, so the deletion is a no-op assertion. |
| Best-effort fallback in `Document::format` (preserve infallibility) may mask convergence bugs in callers that don't use `format_validated`. | Emit a `tracing::warn!` with the divergence summary on every fallback. CLI sets `--strict` to upgrade to `format_validated` semantics. Document gate-via-`format_validated` as the recommended path in `docs/src/`. |
| Pulldown's quirks document (prompt 46) goes stale as `pulldown-cmark` releases land. | Add a CI job that diffs the document's referenced behaviour against a small golden corpus per `pulldown-cmark` release. Failure surfaces as a sweep target for the next prompt. |

## Out of scope

- Replacing `pulldown-cmark`. The bug class is about *agreement* with pulldown's behaviour; a different parser
  trades one disagreement surface for another.
- AST-level structural diff in the gate. Event-stream equivalence is sufficient *and* cheap; an AST diff would
  amplify position-noise into false divergence.
- A custom emphasis tokeniser. The CM §6.2 algorithm is correct; mdwright's job is to produce output that lets
  pulldown's tokeniser reach the same answer as it did on the source.
- Performance work beyond keeping the benches in their envelope. The sweep is correctness-driven; perf wins from
  the simplified call graph (fewer source-byte reads, no defensive normalisations) are bonus.

---

## Appendix A — `Parser::new_ext` audit

Every call site in `src/` as of commit `2e47fbd`. The four production hits are prompt 46's work-list.

| File:line | Caller | Input source | Role | Prompt 46 action |
|---|---|---|---|---|
| `src/document.rs:70` | `render_html` (pub fn) | caller-supplied `&str` (uncanonicalised) | prod | route through `Source::new(source).as_canonical()` |
| `src/ir.rs:264` | `Ir::parse` | post-frontmatter `body` slice of `source` | prod | take `CanonicalSource<'_>` arg; remove `&str` |
| `src/format/emit_safety.rs:76` | `parses_with_outer_run_at` | synthesised `format!("{left}{wrapped}{right}")` | prod (safety ladder) | construct `CanonicalSource` via a `SynthesisedInput` builder |
| `src/format/semantic.rs:167` | gate canonical walker | already-canonicalised input bytes | prod (runtime gate) | take `CanonicalSource<'_>` from caller; delete in-place CR-collapse |
| `src/cm/refs.rs:333` | `parse_events` test helper | test fixture `&str` | `#[cfg(test)]` | leave |
| `src/cm/inline/link.rs:542` | `table_with` test helper | test fixture `&str` | `#[cfg(test)]` | leave |
| `src/cm/inline/code.rs:148` | `reparses_to` round-trip assertion | builder `String` | `#[cfg(debug_assertions)]` | route through chokepoint to share canonicalisation policy |
| `src/format/emit_safety.rs:156` | `parses_as_single_run_isolated` | test bytes | `#[cfg(test)]` | leave |

**Success criterion for prompt 46:** `rg 'pulldown_cmark::Parser::new_ext' src/` shows only call sites inside
`src/source.rs` (the chokepoint module). `#[cfg(test)]` and `#[cfg(debug_assertions)]` sites either route through
the chokepoint or document a per-test reason for not doing so.

## Appendix B — Source-byte decision-read audit

Classification of every `ctx.source` / `tree.raw_text` / `raw_range` / `source_slice` / `source.get` site in
`src/format/` and `src/cm/`. **Decision-read** rows are prompt 47's work-list.

| File:line | Read | Classification | Prompt 47 action |
|---|---|---|---|
| `src/format/inline.rs:74,84` | `ctx.tree.raw_text(ctx.source, cid)` for `emit_emphasis_safely(... source_slice ...)` (Emphasis branch) | **decision-read** (source is the verbatim fallback the ladder reaches when prediction fails) | Replace with draft-bytes-derived fallback in pass 2 |
| `src/format/inline.rs:105,111` | same, Strong branch | **decision-read** | as above |
| `src/format/inline.rs:65-79, 96-110` | `concat_ambient`, `extend_ambient`, `prepend_close` build flank context | **decision-read** (predicts pulldown's view from source neighbours) | Delete; flank read from draft bytes in pass 2 |
| `src/format/inline.rs:114-115, 125-126, 136-137` | strikethrough / link / image ambient strings | **decision-read** | Delete; same as above |
| `src/format/inline.rs:360-413` | paragraph-safety walker duplicate | **decision-read** | Delete; folded into pass 2 |
| `src/format/inline.rs:173, 449` | `text(ctx.tree.raw_text(ctx.source, cid).to_owned())` for non-inline `NodeKind` (defensive debug-assert path) | structural (verbatim payload re-emission) | leave |
| `src/format/inline.rs:148, 425` | `span.pretty(ctx, &node.raw_range)` for `NodeKind::Math` | structural (math body is a verbatim payload) | leave |
| `src/format/block.rs:65` | `node.raw_range.clone()` for CR check inside the post-pass | structural (range scope) | leave |
| `src/format/block.rs:140-152` | HTML/CodeBlock dispatch to `emit_verbatim` | structural (verbatim payload) | leave |
| `src/format/block.rs:168` | `!ctx.source.get(node.raw_range.clone()).unwrap_or("").contains('\r')` — `root_verbatim_safe` | **decision-read** (verbatim-eligibility depends on source CR; output is LF-only by invariant) | Replace with a structural predicate on `Tree::nodes_have_cr_taint` after the IR records taint at parse time |
| `src/format/block.rs:196` | `defs.push((child, node.raw_range.start))` | position-read | leave |
| `src/format/verbatim.rs:30` | `tree.raw_text(source, id)` → emit verbatim | structural | leave |
| `src/format/emit_safety.rs:275, 319` | `source_slice: &str` argument and verbatim fallback | **decision-read** | Becomes draft-derived after prompt 47 |
| `src/cm/block/heading.rs:140` | `ctx.source.get(n.raw_range.clone())` for `split_setext_source` → setext-vs-ATX | **decision-read** (decides emit style from source bytes) | Pass-1 emits IR-shape ATX; pass 2 re-emits setext only when draft confirms the body bytes are setext-safe |
| `src/cm/block/code.rs:112,124` | `ctx.tree.raw_text(ctx.source, id)` for fenced-code body | structural (code body is verbatim payload) | leave |
| `src/cm/block/list.rs:380,387` | `ctx.tree.raw_text(ctx.source, item_id)` for indent derivation | structural (indent measured from source layout) | leave |
| `src/cm/block/list.rs:94,109-110,527-528` | `scan_ordered_delim`, `item_indent` | structural (list marker / indent are properties of the source layout the IR preserves) | leave |
| `src/cm/math/pretty.rs:51` | `let source = ctx.source` for math-body slice | structural (verbatim math payload) | leave |
| `src/cm/block/paragraph.rs:55` | `Paragraph::is_verbatim_eligible(ctx, id)` | structural (decision is on IR shape; the *consequence* is "emit source bytes verbatim") | leave; reads the IR shape, not source bytes |

**Success criterion for prompt 47:** every "decision-read" row in this table moves to "draft-read" or disappears.
`rg 'source_slice|raw_text\(ctx\.source' src/format/inline.rs` returns no production hits.

## Appendix C — `normalize_*` post-pass audit

| Pass | Defined | Wired in | Classification | Prompt 48 action |
|---|---|---|---|---|
| `normalize_trailing_newline` | `src/format/mod.rs:44-56` | `src/format/document.rs:55` | **fold-into-emit** (boundary decision belongs to the last block emitter) | delete; replace with `Block::pretty()` returning a terminator hint per kind |
| `source_has_effective_trailing_newline` | `src/format/mod.rs:61-63` | helper for the above | **fold-into-emit** | delete with `normalize_trailing_newline` |
| `normalize_line_endings_lf` | `src/format/mod.rs:74-80` | `src/format/document.rs:54` + `src/format/semantic.rs:256,407,409,423,425` | **fold-into-emit** (load-bearing invariant lives on `Doc::text` per commit `b4e34dd`; this is defensive) | reduce to `debug_assert!(!out.contains('\r'))` in document.rs; delete the 5 calls in semantic.rs (gate input is already canonical) |
| `apply_end_of_line` | `src/format/mod.rs:87-103` | `src/format/document.rs:56` | **keep-with-rationale** (legitimate output-format policy: Lf/Crlf/Keep, user-facing config) | keep; this is not a defensive normalisation, it is a configured output transform |

**Success criterion for prompt 48:** `rg '^pub\(crate\) fn normalize_' src/format/mod.rs` shows only
`apply_end_of_line`. The five defensive `normalize_line_endings_lf` calls in `src/format/semantic.rs` are gone.
