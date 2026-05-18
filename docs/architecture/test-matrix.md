# Test matrix

mdwright's correctness story sits on five test surfaces. This document maps each surface
to the invariant it defends, where it lives in the tree, and what it does NOT cover. It
exists so a future change to the formatter (or canonicalisation pass) can be assessed
against the right gates without re-deriving them.

## Per-construct golden suites

**Location:** `tests/golden_inline/`, `tests/golden_block/`, `tests/golden_frontmatter/`.

Each fixture is an `*.in` / `*.out` pair. Optional `*.config.toml` overrides
`FmtOptions::default()`. The driver tests live at `tests/golden_inline.rs`,
`tests/golden_block.rs`, `tests/golden_frontmatter.rs` and assert byte equality of the
formatted input against `.out`.

**Invariant:** structural emit and canonicalisation produce the expected bytes for the
exact shapes the project cares about. This is where new features and bugfixes land their
single load-bearing example.

**Does NOT cover:** behaviour on random inputs (property tests do that), behaviour under
options not represented by a `*.config.toml` (the matrix is per-fixture, not per-mode).

## Property tests

**Location:** `tests/properties.rs`, generators at `tests/common/proptest_gen.rs`.

Three families:

| Family | Properties | Cases | Sweep gate |
|---|---|---|---|
| Whole-document, default opts | `idempotent`, `html_preserving`, `lint_preserving`, `reference_resolver_round_trips` | 256 | `*_sweep` at 4096, `#[ignore]` |
| Per-construct, default opts | `<construct>_fragments_idempotent`, `<construct>_fragments_html_preserving` for emphasis, strong, link-inline, link-reference, autolink, code-span, heading, fenced-code, quote, list, table, thematic, footnote | 256 each | none |
| Canonicalisation, 15 modes | `canonicalise_<construct>_semantic_equivalence`, `canonicalise_<construct>_idempotent`, `canonicalise_document_*`. Each iterates `canon_opts()` (preserve + per-knob × variants + 2 all-knobs-together). | 256 × 15 modes | `canonicalise_document_*_sweep` at 4096, `#[ignore]` |

**Invariants tested:**

- **Idempotence:** `format(format(s)) == format(s)` — strict byte equality.
- **HTML preservation / semantic equivalence:** `semantically_equivalent(s, format(s))` —
  canonical pulldown event streams agree.
- **Lint preservation:** `format` does not introduce new default-on diagnostics
  (modulo `bare-url`, which the formatter is allowed to fix into `<...>` autolinks).

**Does NOT cover:** option combinations beyond `canon_opts()`. The two "all-knobs"
modes (`opts_all_asterisk`, `opts_all_underscore_or_dash`) are the cross-knob coverage;
a full Cartesian product would be 4·3·4·3·2·3 = 864 modes and is not pulled in here.

## Regression suite

**Location:** `tests/regressions/`, driver at `tests/regressions.rs`.

Each `*.in` file is a minimal failing input committed in the same change as its fix.
Two gates per fixture:

- `regression_inputs_preserve_html` — `format_validated` must succeed (HTML equivalent
  to source). Skipped for fixtures whose stem ends in `.idem`.
- `regression_inputs_are_idempotent` — byte equality across two format passes. Applied
  to every fixture.

**Invariant:** previously-broken shapes do not re-regress.

**Does NOT cover:** anything not in the file list. Adding a fixture is the way to lock
in a new invariant.

## GFM spec snapshot

**Location:** `tests/gfm_spec.rs`, vendored spec at `tests/gfm-spec/spec.txt`, snapshot
at `tests/gfm-spec/snapshot.txt`.

Two tests:

- `gfm_spec_snapshot` — runs every spec case and compares the residual allowlist
  against `snapshot.txt`. Update with `MDWRIGHT_UPDATE_SNAPSHOT=1`.
- `gfm_spec_coverage` — asserts the bucketing (fully matching / intentional dev /
  tracked regression / unexpected) and refuses any `unexpected` count.

**Invariant:** the formatter's GFM conformance is stable; the snapshot only changes
when intentionally rebaselined.

**Does NOT cover:** behaviour outside the GFM-spec cases. Project-specific extensions
(admonitions, frontmatter, math regions) live in their own golden suites.

## Fuzz oracles

**Location:** `fuzz/fuzz_targets/`. Five targets:

| Target | Oracle | Option byte |
|---|---|---|
| `fuzz_idempotence` | `format(format(s)) == format(s)` | Yes — drives wrap × mode × math × canonicalisation |
| `fuzz_parse_format` | `semantically_equivalent(s, format(s))` | Yes — same allocation as `fuzz_idempotence` |
| `fuzz_structured_idempotence` | Structural-only idempotence with `FormatMode::Verbatim` excluded | No |
| `fuzz_verbatim_identity` | `FormatMode::Verbatim` emits source byte-for-byte | No |
| `fuzz_lint` | Formatter does not invent new default-on diagnostics | No |

**Option byte allocation** (`fuzz_idempotence` and `fuzz_parse_format`, identical):

| Bits  | Field |
|-------|-------|
| 0–1   | `wrap` (Keep, No, At(80), At(120)) |
| 2     | `math.normalise` |
| 3     | `mode` (Normalise, Verbatim) |
| 4–7   | Canonicalisation mode (16 enumerated: preserve, one per style knob, two combined) |

**Invariant:** no input causes a panic or property violation in 10 minutes (the
reference budget the prompt-49 reverification used). Findings are committed to
`tests/regressions/` as `.in` fixtures.

**Does NOT cover:** behaviour beyond `MAX_INPUT = 65 536` bytes; the libFuzzer harness
skips bigger inputs. The CLI enforces the same shape via `--max-input-bytes`.

## How to choose what to add when

| Symptom | Right surface |
|---|---|
| One specific fixture or shape misbehaves | Golden suite (add an `*.in` / `*.out` pair) |
| A bug class spans many inputs of one construct | Per-construct property (a new `<construct>_fragments_*` pair, or strengthen the existing one) |
| A canonicalisation mode misbehaves | Canonicalisation property (extend `canon_opts()`) |
| A minimal counterexample of a property failure surfaces | Regression suite (`*.in` next to the fix commit) |
| GFM conformance shifts | Audit `gfm_spec_coverage` first, then rebaseline the snapshot with a comment line above each new entry |
| Pathological inputs reach a panic / property violation | Add the input as a regression fixture; libFuzzer will not re-find it once it round-trips |

## What this matrix does NOT include

Lint-rule coverage lives with each rule under `src/stdlib/*` and `tests/`; that's a
parallel matrix and isn't summarised here. CLI-surface tests live at
`tests/cli_*.rs`. The diagnostic JSON v2 schema is gated by
`tests/diagnostic_json_v2.rs`.
