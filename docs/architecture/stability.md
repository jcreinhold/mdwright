# Stability charter

> **Invariant.** Every output of `Document::format` is byte-equivalent to its input wherever the input was already
> valid Markdown, and semantically equivalent (canonical pulldown event streams agree) for any input under any
> `FmtOptions`. The runtime gate at `Document::format_validated` enforces the semantic half; the structural half is
> a property of every `.pretty()` method individually.

> **Sweep status.** Prompts 51–55 (v0.4.0) replaced the iterative-draft + safety-ladder design from prompt 47 with
> a two-stage pipeline: structural emit reads source bytes only (preserve-by-default, idempotent by construction)
> and a separate post-pass at `src/format/canonicalise.rs` rewrites bytes per `FmtOptions` style knobs, verifying
> each rewrite locally. The two-pass convergence loop, `FlankSource`, `DraftView`, the safety ladder, the
> `ConvergenceError` / `FormatError::DidNotConverge` pair, and `Tree::corresponding_node_map` are gone (~800 lines
> deleted across prompts 51–52). This document reflects post-sweep state. Prompts 46 (chokepoint) and 47
> (iterative-draft, since superseded) stay below as history. Original prompts 48–49 are **superseded** by the
> 51–55 sweep; the redesign achieved their goals (`normalize_trailing_newline` retained for the legitimate
> boundary case, fixed-point gate replaced by per-construct preservation).

mdwright's correctness today rests on three architectural choices, each one a deep module rather than a layered
agreement between consumers:

1. **One pulldown chokepoint** at `src/parse.rs::events` / `events_with_offsets`. Every `pulldown_cmark::Parser`
   construction in production code routes through this one site. Pulldown quirks live in
   `docs/architecture/pulldown-model.md`, drift-tested by `tests/pulldown_model.rs`.

2. **Structural emit is pure source-byte preservation.** Every `.pretty()` method reads source bytes through
   `Tree::raw_text` or a parse-time-recorded field; none consult `FmtOptions` style knobs. Idempotent by
   construction.

3. **Style canonicalisation is a separate verified post-pass.** Opt-in via `FmtOptions` style knobs; default is
   `Preserve` everywhere. Each rewrite reparses a paragraph window through the chokepoint and skips silently
   when the parse would diverge. The pass iterates internally to a fixed point.

The bug class that motivated the redesign — emit decisions that perturbed their own context, requiring a
convergence loop and per-site safety ladder to recover — is unrepresentable: there is no decision point that
reads source bytes to predict pulldown's behaviour.

## The bug class *[historical context]*

Three round-2 fixes after prompt 44 — `36ded18` (`oracle-domain`), `223cd28` (`boundary-newline-policy`),
`0b5eaf7` (`emphasis-flank-oscillation`) — were local patches to instances of the same shape: a downstream pass
*predicted* what pulldown would do, instead of asking pulldown what it does. Each fix was correct; none addressed
the shape.

Round-3 verification immediately produced two more findings in the same family. `_*/*_` (5 bytes) — pulldown sees
nested emphasis; pre-v0.4.0 mdwright emitted `*\*/\**` (one outer emphasis, escaped body), which re-parsed to a
single emphasis and failed the gate. `**u*~***~` — pulldown sees one Strong wrapping Emphasis-and-text plus
trailing literals; pre-v0.4.0 mdwright produced `**u*~*\*\*~` on pass 1 and `**u*~~\*\*\*~~` on pass 2.

See [`fuzz-history.md`](fuzz-history.md): 9 of the last 22 pre-v0.4.0 fuzz fixes belonged to the
"output-decision consults source bytes" pattern. Per-finding patches were a treadmill.

The redesign reframes the underlying cause: as long as any emit site reads source bytes to choose its
representation, perturbation is possible. Removing that read site (preserving source representation byte-for-byte)
makes the bug class disappear.

## The architectural moves

### Prompt 46 — Canonical-source chokepoint + pulldown-quirks model *[landed]*

Every `pulldown_cmark::Parser` invocation in `src/` goes through `src/parse.rs::events` (or
`events_with_offsets`), both of which take a `CanonicalSource<'_>` (`src/source.rs`). The newtype's only public
constructor (`CanonicalSource::from_source`) routes through `Source::canonicalise`, so the type system enforces the
chokepoint discipline. Verified: `rg 'Parser::new_ext|Parser::new\(' src/` returns exactly two hits, both in
`src/parse.rs`.

`docs/architecture/pulldown-model.md` documents the per-construct invariants the formatter relies on. Drift-tested
by `tests/pulldown_model.rs`: one test per rule, each failing with a message that names the doc section to update
*before* changing mdwright code.

Side benefits: the per-event CR scrub in `format::semantic::canonical_events` is gone (input is provably CR-free);
the per-site `Options::empty() + insert()` boilerplate collapses to one `parse::FORMATTER_OPTIONS` constant.

### Prompt 47 — Output-derived emit (iterative-draft) *[landed as stepping stone; superseded]*

Prompt 47 shipped a two-pass formatter with `FlankSource::Draft(view)`, where pass 2's emit decisions read the
draft bytes pass 1 produced (rather than predicting from source). The mechanism worked but addressed a symptom
rather than the root cause: emit decisions still depended on neighbouring bytes; they just consulted a more
reliable neighbour. Round-3 findings persisted because the pass-2 draft confirmed each *local* decision was correct
without verifying that the rendered bytes preserved the source's nested-IR shape.

Prompt 51 collapsed the entire design space by making structural emit pure source-byte preservation. With no emit
site choosing a representation, the convergence loop and safety ladder became unreachable; prompt 52 deleted
both. See the prompts 51–55 sweep below.

### Prompts 48–49 (original) — **superseded by the 51–55 sweep**

The original prompts 48 (structural emission of trailing newline) and 49 (fixed-point gate + redundant-ladder
deletion) were both made unnecessary by the structural-preserve redesign. `normalize_trailing_newline` is kept
as-is — it is a legitimate boundary policy reading source bytes to decide whether the emitted document should
end with `\n`, *not* a perturbation source — and the fixed-point gate is no longer needed because every
`.pretty()` method is byte-preserving (idempotent by construction). The redundant-ladder deletion happened in
prompt 52 alongside the safety-ladder removal.

### Prompts 51–55 — Structural-preserve redesign sweep *[landed, v0.4.0]*

- **Prompt 51 — Defaults flip; per-construct emit is pure preservation.** Every `.pretty()` method changed to
  read source bytes via `Tree::raw_text` / parse-time-recorded fields. `FmtOptions` style knob defaults all
  flipped to `Preserve`. New `ThematicStyle::Preserve` / `LinkDefStyle::Preserve` variants. The bar:
  `rg 'opts\.(italic|strong|list_marker|thematic|link_def|ordered_list)' src/cm/` returns nothing.

- **Prompt 52 — Deletion sweep.** `src/format/emit_safety.rs` (~474 lines), `FlankSource` / `DraftView` /
  `FlankCtx`, `Tree::corresponding_node_map`, `format::ConvergenceError`, `FormatError::DidNotConverge`,
  `verbatim_source_fallback`, the `MAX_PASSES` loop, and the `verbatim_source_fallback` path all deleted.
  `format_document` is a single-pass function returning `String` directly.

- **Prompt 53 — Separate canonicalisation pass.** New `src/format/canonicalise.rs` with per-rewrite verification
  through the prompt-46 chokepoint. One rewriter per `FmtOptions` style knob; failed verifications skip silently
  with `tracing::warn!`. Gated by `opts.has_any_canonicalisation()` so default config pays zero cost. New
  `StrongStyle` knob, independent of `ItalicStyle`.

- **Prompt 54 — Property matrix + fuzz reverify + round-3 promotion.** Property tests at
  `tests/properties.rs` matrix every style knob (15 modes × {byte idempotence, semantic equivalence}) at 256
  cases by default and 4096 cases under `#[ignore]`. Fuzz harnesses extend their first-byte option encoding to
  cover the canonicalisation matrix. Round-3 fixtures promoted to
  `tests/regressions/fuzz_round3_*.in`. Four structural-emit residuals fixed mid-prompt (escape policy,
  frontmatter, empty blockquote, canonicalise convergence). Two further pre-existing structural-emit edge cases
  documented at `docs/architecture/round-4-findings/`.

- **Prompt 55 — Documentation, charter rewrite, memory hygiene.** This document and the surrounding doc
  surfaces brought into sync with the post-sweep code.

## Architecture diagram

After the sweep, the format pipeline is linear with one conditional branch:

```
raw &str
    │
    ▼
Source::new ── canonicalise ──► CanonicalSource(&str)
                                        │
                                        ▼
                              pulldown::Parser (sole call site)
                                        │
                                        ▼
                                  typed IR (Tree, math overlay, refs)
                                        │
                                        ▼
                        structural emit (per-construct .pretty()
                        methods — pure source-byte preservation)
                                        │
                                        ▼
                          normalize_line_endings_lf
                                        │
                                        ▼
                          if opts.has_any_canonicalisation():
                              canonicalise (per-rewrite verified,
                              iterated internally to fixed point)
                                        │
                                        ▼
                          normalize_trailing_newline
                                        │
                                        ▼
                          apply_end_of_line
                                        │
                                        ▼
                                      out
```

Every arrow that crosses a type boundary is enforced by the type of its source: only `Source::canonical()`
produces a `CanonicalSource`; only `format::format_document` calls `pulldown::Parser` via the chokepoint. The
only surviving crate-internal newtype from the prompt-46 era is `CanonicalSource`. `FlankSource`, `DraftView`,
`DraftOutput`, `ConvergedOutput` were all deleted with the safety ladder in prompt 52.

## Public API contract

- `Document::parse(source: &str) -> Document` — unchanged. Still infallible.
- `Document::format(&self, opts: &FmtOptions) -> String` — unchanged signature, infallible.
- `Document::format_validated(&self, opts: &FmtOptions) -> Result<String, FormatError>` — `FormatError` carries
  only the `SemanticDivergence { formatted, diff_summary, html_a, html_b }` variant. The pre-sweep
  `DidNotConverge` variant is gone; the convergence loop it signalled does not exist.
- `mdwright_format::semantically_equivalent(a: &str, b: &str) -> bool` — unchanged.
- `FmtOptions` style knobs default to `Preserve`. Six new fluent setters (`with_italic`, `with_strong`,
  `with_list_marker`, `with_ordered_list`, `with_thematic_break`, `with_link_def_style`) for programmatic
  callers. New `[fmt] strong = "..."` TOML key. New `[fmt] thematic-break = "..."` TOML key.

The mdBook docs at `docs/src/format/policy.md` and `docs/src/format/style.md` describe the user-facing surface.

## Risk register

| Risk | Mitigation |
|---|---|
| Canonicalisation pass's internal convergence loop could fail to terminate. | Capped at `MAX_CANONICALISE_ITERS = 8`; cap exceedance logs a `tracing::warn!` and returns the current buffer. The 4096-case property sweep at `tests/properties.rs::canonicalise_document_*_sweep` has never hit the cap. |
| Per-rewrite verification's paragraph-window scope is too small for some rewrites. | Conservative by design: rewrites that would affect adjacent paragraphs verify within their own window and skip if the local parse diverges, leaving the source-preserved bytes in place. Skips are logged so production traffic can surface high-skip-rate documents. |
| Structural emit edge cases not covered by the 4096-case sweep show up in pathological inputs. | `docs/architecture/round-4-findings/` tracks the two known cases (empty list item at end-of-document; ATX trailing-hash). Both reproduce under `FmtOptions::default()`; both are pre-existing structural-emit bugs that fuzz surfaces via the broader option-space coverage. Future structural-emit work uses these as the acceptance suite. |
| Pulldown's behaviour drifts between releases. | `docs/architecture/pulldown-model.md` documents the per-construct invariants; `tests/pulldown_model.rs` fails when pulldown's behaviour disagrees. The chokepoint at `src/parse.rs` is the one site any drift mitigation lands. |

## Out of scope

- Replacing `pulldown-cmark`. The bug class is about *agreement* with pulldown's behaviour; a different parser
  trades one disagreement surface for another.
- AST-level structural diff in the gate. Event-stream equivalence is sufficient *and* cheap; an AST diff would
  amplify position-noise into false divergence.
- A custom emphasis tokeniser. The CM §6.2 algorithm is correct; mdwright's job is to produce output that lets
  pulldown's tokeniser reach the same answer as it did on the source.
- Cross-knob canonicalisation modes beyond what `FmtOptions` exposes. For aggressive cross-knob normalisation
  (mdformat's approach), use mdformat — see the README.

---

## Appendix A — `Parser::new_ext` audit *[closed by prompt 46]*

Every production call site routes through `src/parse.rs::events` or `events_with_offsets` (which both call
`Parser::new_ext` internally — the only two production sites). `#[cfg(test)]` helpers in `src/cm/inline/link.rs`
and similar locations have explicit per-test reasons documented next to the call. The audit is closed.

## Appendix B — Source-byte decision-read audit *[closed by prompt 51 + 53]*

The prompt-47 audit identified every site where the formatter read source bytes to *decide* its emit shape (as
opposed to *copying* a source payload verbatim). Every "decision-read" row has either been:

- **Eliminated by structural-preserve** (prompt 51): the decision was deferred to source bytes anyway, so the
  read became a copy. `cm/inline/link.rs::body_text_for_decision`, `cm/block/heading.rs::split_setext_source`,
  `cm/block/paragraph.rs::Paragraph::is_verbatim_eligible`, the per-emit-site emphasis flank reads in
  `format/inline.rs` — all are gone, with structural emit picking the source representation directly.
- **Moved to the canonicalisation pass** (prompt 53): the decision is a deliberate user-requested rewrite,
  verified locally by a paragraph-window reparse. `src/format/canonicalise.rs` is the only consumer of
  `FmtOptions` style knobs.

The audit is closed. The new bar: `rg 'opts\.(italic|strong|list_marker|thematic|link_def|ordered_list)' src/`
returns only the call sites in `src/format/canonicalise.rs`.

## Appendix C — `normalize_*` post-pass audit *[partially closed]*

| Pass | Defined | Wired in | Status |
|---|---|---|---|
| `normalize_trailing_newline` | `src/format/mod.rs` | `src/format/document.rs` | **kept.** Originally slated for deletion in prompt 48; the structural-preserve sweep made the perturbation concern moot, and the function is a legitimate boundary policy (read source bytes to decide whether output should end with `\n`). |
| `source_has_effective_trailing_newline` | `src/format/mod.rs` | helper for the above | **kept** with `normalize_trailing_newline`. |
| `normalize_line_endings_lf` | `src/format/mod.rs` | `src/format/document.rs` | **kept** as a cheap belt-and-braces. The load-bearing invariant lives on `Doc::Text`'s construction. |
| `apply_end_of_line` | `src/format/mod.rs` | `src/format/document.rs` | **kept** — configured output transform. |

No defensive `normalize_line_endings_lf` calls remain in `src/format/semantic.rs` (deleted alongside the per-event
CR scrub in prompt 46).
