# Fuzz-fix history

Catalogue of every fuzz-driven fix landed since the first `fuzz`-zero milestone, classified by the architectural pattern
it belongs to. Patterns are numbered to match the concerns enumerated in [`stability.md`](stability.md).

The dominant pattern, by a wide margin, is **#2: an output decision consulted source bytes to choose a representation.**
9 of 22 classified fixes fall here. That concentration is why structural emit is now identity (the formatter copies
source bytes for the structural skeleton instead of re-deriving them): removing the read-to-decide site removes the bug
class. The opt-in canonicalisation path is hardened separately — rewrite families own normal-form plans with explicit
byte ownership, and verification checks safety before commit. See
[`formatter-rewrite-boundary.md`](formatter-rewrite-boundary.md) for the current emit shape.

## Histogram

| Pattern | Fixes | Resolution                                                                                                |
| ------- | ----- | --------------------------------------------------------------------------------------------------------- |
| #2 output decision consults source bytes | 9 | Source-preserving emit reads source bytes only to copy them; style choices live in verified rewrite families. |
| #1 chokepoint missing (`Parser::new_ext` scattered) | 5 | Every call site routes through `CanonicalSource`.                                  |
| #3 post-pass at wrong layer              | 3 | Subsumed by identity structural emit; perturbation concern is moot.                              |
| #6 safety-ladder fallback redundancy     | 1 | Safety ladder deleted; structural emit is byte-preserving by construction.                       |
| #4 runtime gate weaker than tests        | 0 (CI fuzz only) | Subsumed by identity structural emit; preserve-by-emit is idempotent by construction.     |

The histogram excludes three classes as "not a sweep target": `upstream-pulldown-panic` (external, contained at the
document boundary), `list-marker-derivation` (local recogniser fix), and a handful of paragraph-continuation IR fixes.

## Fixes by cause class

Tagged commits use the `cause-class:` convention; older commits predate it and are classified retroactively from their
message bodies.

### #2 source-derived emit decisions (9)

| Commit                      | Layer                                                | Note                                                          |
| --------------------------- | ---------------------------------------------------- | ------------------------------------------------------------- |
| `9f82b3c`                   | `emit-safety: per-emphasis fallback ladder`          | Closed bug class A; also introduced the ladder (#6).          |
| `9fe379d`                   | `emit-safety: source flank to validation`            | Other-delim fallback.                                          |
| `961fcd0`                   | `code: context-aware round-trip check`               | Closed bug class B.                                            |
| `3a0abc8`                   | `heading: decide setext-vs-ATX from source bytes`    | Source-byte-decision-read.                                     |
| `de5895e`                   | `strikethrough: escape interior ~ for ~~ body ~~`    | Output-aware escape.                                           |
| `77db28b`                   | `code: fix code-span padding inflation`              | Encoded round-trip invariant.                                  |
| `5c21892`                   | `src/format/emit_safety.rs` (widen flank)            | Widened flank to enclosing block.                              |
| `0b5eaf7`                   | `src/format/inline.rs` (ambient threading)           | `emphasis-flank-oscillation`.                                  |
| Round-3 fixtures (`_*/*_`, `**u*~***~`) | identity structural emit                 | Both byte-preserve under current defaults; promoted to `tests/regressions/`. |

### #1 chokepoint missing (5)

| Commit                            | Layer                                                      |
| --------------------------------- | ---------------------------------------------------------- |
| `e9e8fba`                         | `src/format/semantic.rs` (CR collapse in verbatim events). |
| `32b64d4`                         | CLI + fuzz pre-filter (`--reject-control-chars`).          |
| `36ded18`                         | `src/format/semantic.rs` (gate CR-normalises in place).    |
| `13259ae` / `1f68693`             | `Source`/`ByteSpan`/`OriginalSpan`/`OffsetMap` foundation. |
| `520572e` / `5d63f2a`             | LF-only rendered bytes; CR-refusal at root verbatim gate.  |

### #3 post-pass at wrong layer (3)

| Commit       | Layer                                              |
| ------------ | -------------------------------------------------- |
| `b4e34dd`    | `Doc::Text` canonicalises CR at construction.      |
| `223cd28`    | `src/format/mod.rs::normalize_trailing_newline`.   |
| `520572e`    | LF-only rendered bytes (also #1).                   |

### Contained upstream panics

`7410d6d` and the `mdwright-document::ParseError` boundary convert upstream pulldown panics into typed errors;
reproducers live in `crates/mdwright-document/src/known_issues.rs`.
