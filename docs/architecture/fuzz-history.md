# Fuzz-fix history

Every fuzz-driven fix landed since prompt 32 (the first `fuzz`-zero milestone), with the layer it modified and the
architectural pattern it belongs to. The pattern column maps each fix to one of the six concerns enumerated in
[`stability.md`](stability.md). Tagged commits use the `cause-class:` convention introduced after prompt 44; older
commits predate the convention and are tagged here retroactively from their commit-message body.

The aggregate shape: **8 of the last 11 fuzz fixes** belong to patterns #1–#2 (`Parser::new_ext` chokepoint missing,
or output decisions consulting source bytes). That is the evidence the 45-49 sweep rests on.

## Round 3 (post-round-2, evidence only — not yet fixed)

| Finding | Target | Pattern |
|---|---|---|
| `01-parse-format-nested-emphasis-with-slash.in` (`_*/*_`) | `fuzz_parse_format` | #2 source-derived flank |
| `02-idempotence-emphasis-strong-strikethrough.in` (`**u*~***~`) | `fuzz_idempotence` | #2 + multi-construct sibling interaction |

Recorded at `docs/architecture/round-3-findings/`. Prompt 49 promotes these to `tests/regressions/` after the sweep
makes them pass.

## Round 2 (after prompt 44; cause-class convention in use)

| Commit | Cause-class | Layer | Pattern |
|---|---|---|---|
| `0b5eaf7` | `emphasis-flank-oscillation` | `src/format/inline.rs` (ambient threading) | #2 source-derived flank |
| `223cd28` | `boundary-newline-policy` | `src/format/mod.rs::normalize_trailing_newline` | #3 post-pass at wrong layer |
| `36ded18` | `oracle-domain` | `src/format/semantic.rs` (gate CR-normalises in place) | #1 chokepoint missing |

## Round 1 (prompt 44 sprint to fuzz-zero)

| Commit | Cause-class | Layer | Pattern |
|---|---|---|---|
| `7410d6d` / Phase 59 | `upstream-pulldown-panic` | `mdwright-document::ParseError` containment + `known_issues.rs` | external (pulldown) contained at document boundary |
| `5c21892` | `emphasis-pairing-context` | `src/format/emit_safety.rs` (widen flank to enclosing block) | #2 source-derived flank |
| `e9e8fba` | `oracle-domain` | `src/format/semantic.rs` (CR collapse in verbatim events) | #1 chokepoint missing |
| `efb9d6a` | `list-marker-derivation` | `src/ir.rs` (scan for legal marker class) | local recogniser fix — not a sweep target |
| `32b64d4` | `oracle-domain` | CLI + fuzz pre-filter (`--reject-control-chars`) | #1 chokepoint missing (input domain) |

## Pre-tag era (prompts 32-43, retroactively classified)

The cause-class tag was introduced post-prompt-44; older fixes carry the same patterns. Sampled chronologically;
this is not exhaustive but shows the same shape was already dominant.

| Commit | One-line | Pattern |
|---|---|---|
| `9f82b3c` | `emit-safety: per-emphasis fallback ladder closes bug class A` | #2 + #6 (ladder introduced here) |
| `9fe379d` | `emit-safety: pass source flank to validation; add other-delim fallback` | #2 source-derived flank |
| `961fcd0` | `code: context-aware round-trip check closes bug class B` | #2 source-derived decision |
| `13259ae` / `1f68693` | `source: introduce Source/ByteSpan/OriginalSpan/OffsetMap foundation` + `document: own Source` | #1 partial chokepoint (introduced) |
| `b4e34dd` | `doc: canonicalise CR at Doc::Text construction; setext widths agree` | #3 (defensive normalisation pushed down) |
| `3a0abc8` | `heading: decide setext-vs-ATX from source bytes, not rendered Doc` | #2 source-derived decision |
| `520572e` / `5d63f2a` | `format: enforce LF-only rendered bytes` + `lift CR-refusal to root verbatim gate` | #3 + #1 |
| `de5895e` | `strikethrough: escape interior ~ to make ~~ body ~~ round-trip` | #2 (output-aware escape) |
| `77db28b` | `code: fix code-span padding inflation; encode round-trip invariant` | #2 |
| `e8d0f1e` / `bf0c794` | paragraph continuation / ParagraphBody constructor work | local IR fix — not a sweep target |
| `ee22520` | `format: invert Doc::Text atomicity; switch gate to event-stream equivalence` | refactor that made the #4 gap visible |

## Pattern histogram

Tallied across rounds 1-3 plus the eleven sampled pre-tag fixes (22 entries total; the `upstream-pulldown-panic`,
`list-marker-derivation`, and local IR fixes are excluded as "not a sweep target"):

| Pattern | Count | What the sweep does |
|---|---|---|
| #1 chokepoint missing (`Parser::new_ext` scattered) | 5 | prompt 46 lifts every site through `CanonicalSource` |
| #2 output decision consults source bytes | 9 | prompts 51–52 delete decision-reads entirely: every `.pretty()` reads source bytes only to copy them, never to decide a representation |
| #3 post-pass at wrong layer | 3 | superseded; structural-preserve makes the perturbation concern moot |
| #4 runtime gate weaker than tests | 0 fixes (only seen via CI fuzz) | superseded; preserve-by-emit is idempotent by construction |
| #6 safety-ladder fallback redundancy | 1 (the introduction) | prompt 52 deletes the safety ladder entirely |

#2 alone is 41 % of the fix history — the dominant pattern. The prompt-47 iterative-draft formatter was the
first attempt to address it (read pass-1 draft bytes for emit decisions rather than source bytes); it shipped
and worked, but a follow-up audit found the decisions could still drift via nested-IR-shape interactions that
the per-site safety ladder did not cover. The prompts 51–55 sweep replaced both with structural-preserve emit
(`.pretty()` methods do not choose a representation; they copy the source's) plus a separately verified
canonicalisation pass (`src/format/canonicalise.rs`) for opt-in rewrites. See
[`stability.md`](stability.md) for the post-sweep architecture.
