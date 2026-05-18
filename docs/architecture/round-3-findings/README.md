# Round-3 fuzz findings — historical record

These two inputs were discovered in a round-3 fuzz verification (5 min/target) after the prompt-44 patches and the
round-2 follow-up. They drove the design of the prompt-51-through-55 redesign sweep (the structural-preserve +
separate-canonicalisation architecture documented in [`../stability.md`](../stability.md)).

## Where they live now

- `01-parse-format-nested-emphasis-with-slash.in` → `tests/regressions/fuzz_round3_nested_emphasis_slash.in`
- `02-idempotence-emphasis-strong-strikethrough.in` → `tests/regressions/fuzz_round3_multi_construct_idempotence.in`

Both byte-preserve under v0.4.0 structural-preserve defaults. The promotion happened in prompt 54 (`42da3a6`); the
regression harness at `tests/regressions.rs` keeps them green from that commit onward.

## What each shows

### 01 — `_*/*_` (5 bytes)

Pulldown event stream: `Start(Emphasis 0..5), Start(Emphasis 1..4), Text("/"), End(Emphasis), End(Emphasis)` —
that is, `Emphasis(Emphasis("/"))`.

Pre-v0.4.0 mdwright output: `*\*/\**` — one `Emphasis` whose body was the literal text `*/*`. The inner emphasis
got flattened to escaped text; re-parse gave `Emphasis(Text("\\*/\\*"))`, not `Emphasis(Emphasis(...))`, so
`semantically_equivalent` reported divergence. Same shape as GFM-spec example 470 (`*_foo_*`) but the body byte
(`/`) tripped the safety ladder's embedded-reparse decisions in a way the round-2 ambient-threading workaround did
not anticipate.

Under v0.4.0: every `.pretty()` method reads source bytes; the outer `_…_` and inner `*…*` both round-trip
verbatim. The nested-IR shape survives by construction.

### 02 — `!**u*~***~` (10 bytes: option byte `0x21` + source `**u*~***~`)

Option byte `0x21` selects `Wrap::No`, `FormatMode::Normalise`, `math.normalise = false` under the pre-v0.4.0
fuzz harness encoding.

Pulldown event stream for `**u*~***~`: `Emphasis(Emphasis("u") + Text("~")) + Text("*") + Text("*") + Text("~")`.
The two trailing `~` characters in the input are *not* a strikethrough pair (pulldown's pairing decision keeps
them as plain text, because the outer Emphasis closes at byte 5 and consumes the `*` flanking that would have
extended the run).

Pre-v0.4.0 mdwright:
- Pass 1 → `**u*~*\*\*~`
- Pass 2 → `**u*~~\*\*\*~~`

Pass 1's escape policy escaped the trailing `**` text to `\*\*`, which then formed a strikethrough pair with
the surrounding `~` characters on re-parse (because pulldown's `~`/`~~` pairing depends on the *flanking* of the
chars between, and the escapes changed that flanking). Pass 2 saw the new strikethrough and re-emitted yet
again. Non-idempotent.

Under v0.4.0: the escape policy at `src/cm/inline/escape_policy.rs::needs_emphasis_escape` requires a
non-delimiter body byte between paired delimiters before considering them an emphasis candidate. The trailing
`**` text no longer gets escaped, the `~`-pair flanking stays as in the source, and the input round-trips.

## Why the sweep, not a per-finding patch

Both findings landed *after* the round-2 fixes addressed the previous round of failures. The pattern was clear:
emit decisions that consulted source bytes to predict pulldown's behaviour were the recurring source. The
redesign sweep (prompts 51–55) addressed the pattern at the architectural level: structural emit cannot perturb
its own context because it does not choose a representation; the canonicalisation pass cannot drift globally
because each rewrite verifies its own parse window.

See [`../stability.md`](../stability.md) for the full redesign narrative.
