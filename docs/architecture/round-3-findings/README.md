# Round-3 fuzz findings — evidence for prompts 45–49

After prompt 44 (initial fuzz-to-zero) and the round-2 follow-up (oracle-domain CR, boundary-newline-policy,
emphasis-flank-oscillation), a round-3 verification at 5 min/target produced two more findings in the same
architectural family. These are **not** seeded as regression tests (that would block the build); they are evidence
that the 45–49 architectural sweep is necessary and concrete inputs that prompt 49's fuzz re-verification must
clear.

| # | File | Target | Class |
|---|---|---|---|
| 01 | `01-parse-format-nested-emphasis-with-slash.in` | `fuzz_parse_format` | Nested emphasis `_*…*_` collapses to single emphasis on emit; semantic divergence. |
| 02 | `02-idempotence-emphasis-strong-strikethrough.in` | `fuzz_idempotence` | Multi-construct (Strong/Emphasis/Strikethrough) emit decisions interact; pass 2 ≠ pass 1. |

## What each shows

### 01 — `_*/*_` (5 bytes)

Pulldown event stream: `Start(Emphasis 0..5), Start(Emphasis 1..4), Text("/"), End(Emphasis), End(Emphasis)` —
that is, `Emphasis(Emphasis("/"))`.

Current mdwright output: `*\*/\**` — one `Emphasis` whose body is the literal text `*/*`. The inner emphasis was
flattened to escaped text. Re-parse gives `Emphasis(Text("\\*/\\*"))`, not `Emphasis(Emphasis(...))`, so
`semantically_equivalent` reports divergence.

This is the same shape as GFM-spec example 470 (`*_foo_*`), which the round-2 fix *does* handle correctly via
ambient-threading. The difference here is the body byte (`/`) — the safety ladder's embedded-reparse decisions are
sensitive to body content in a way the ambient workaround did not anticipate. Prompt 47's two-pass design eliminates
this by construction: pass 2 reads the actual draft bytes for the flank, not an approximated ambient string.

### 02 — `!**u*~***~` (10 bytes: opt 0x21 + source `**u*~***~`)

Option byte `0x21` selects `Wrap::No`, `FormatMode::Normalise`, `math.normalise = false`.

Pulldown event stream for `**u*~***~`: `Strong(Emphasis("u") + Text("~")) + Text("*") + Text("*") + Text("~")`.

mdwright passes:
- Pass 1 → `**u*~*\*\*~`
- Pass 2 → `**u*~~\*\*\*~~`

Pass 2 reads pass 1's output as a different IR (`~~`-pairs interact differently with the trailing text), and re-emits.
Non-idempotent.

This is precisely the multi-construct case where the round-2 ambient threading is insufficient: ambient bytes for an
emphasis emit don't account for following siblings that will also be rewritten. Prompt 47's two-pass design naturally
handles it (pass 2 reads the full draft, including the formatter's rendering of every sibling).

## How prompts 45–49 must use these

- **Prompt 45 (charter):** cite as fresh evidence in `docs/architecture/stability.md`'s "The bug class" section.
  These two findings landed *after* the round-2 fixes, which validates the charter's claim that per-finding patches
  are insufficient.
- **Prompt 47 (two-pass formatter):** add both inputs to the verification block. The two-pass design must produce
  semantically-equivalent + idempotent output for both. If it doesn't, the design is incomplete and the prompt
  needs revision.
- **Prompt 49 (fixed-point gate + re-verify):** promote these from `docs/architecture/round-3-findings/` to
  `tests/regressions/fuzz_round3_*.in` after the sweep makes them pass. The promotion commit's diff is the proof
  that the sweep accomplished what it set out to do.
