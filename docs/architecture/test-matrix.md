# Test matrix

mdwright's correctness sits on these test surfaces. For each: the invariant it defends, where it lives, and what it does
NOT cover. Use this to decide which gate(s) a change to the formatter or canonicalisation pass needs to clear.

## Per-construct golden suites

**Location:** `tests/golden_inline/`, `tests/golden_block/`, `tests/golden_frontmatter/`.

Each fixture is an `*.in` / `*.out` pair. Optional `*.config.toml` overrides `FmtOptions::default()`. The driver tests
live at `tests/golden_inline.rs`, `tests/golden_block.rs`, `tests/golden_frontmatter.rs` and assert byte equality of the
formatted input against `.out`.

**Invariant:** structural emit and canonicalisation produce the expected bytes for the exact shapes the project cares
about. This is where new features and bugfixes land their single load-bearing example.

**Does NOT cover:** behaviour on random inputs (property tests do that), behaviour under options not represented by a
`*.config.toml` (the matrix is per-fixture, not per-mode).

## Property tests

**Location:** `tests/properties.rs`, generators at `tests/common/proptest_gen.rs`.

Four families:

| Family | Properties | Cases | Sweep gate |
|---|---|---|---|
| Whole-document, default opts | `idempotent`, `html_preserving`, `lint_preserving`, `reference_resolver_round_trips` | 256 | `*_sweep` at 4096, `#[ignore]` |
| Per-construct, default opts | `<construct>_fragments_idempotent`, `<construct>_fragments_html_preserving` for emphasis, strong, link-inline, link-reference, autolink, code-span, heading, fenced-code, quote, list, table, thematic, footnote | 256 each | none |
| Canonicalisation, 15 modes | `canonicalise_<construct>_semantic_equivalence`, `canonicalise_<construct>_idempotent`, `canonicalise_document_*`. Each iterates `canon_opts()` (preserve + per-knob × variants + 2 all-knobs-together). | 256 × 15 modes | `canonicalise_document_*_sweep` at 4096, `#[ignore]` |
| Rewrite-law interactions | `*_interactions_are_profile_idempotent` for nested lists, nested inline slots, tables with inline content, wrapped paragraphs with atomics, link destinations, math, and frontmatter. Each iterates preserve, mdformat, known fuzz profiles, and an all-family profile. | 96 × 5 profiles | none |

**Invariants tested:**

- **Idempotence:** `format(format(s)) == format(s)`: strict byte equality.
- **Rewrite-law completion:** the second pass over generated rewrite-interaction inputs commits no rewrites; family
  planning must reach its normal form in the first public format call.
- **HTML preservation / semantic equivalence:** `semantically_equivalent(s, format(s))`: canonical pulldown event
  streams agree.
- **Lint preservation:** `format` does not introduce new default-on diagnostics (modulo `bare-url`, which the formatter
  is allowed to fix into `<...>` autolinks).

**Does NOT cover:** option combinations beyond `canon_opts()`. The two "all-knobs" modes (`opts_all_asterisk`,
`opts_all_underscore_or_dash`) are the cross-knob coverage; a full Cartesian product would be 4·3·4·3·2·3 = 864 modes
and is not pulled in here.

## Regression suite

**Location:** `tests/regressions/`, driver at `tests/regressions.rs`.

Each `*.in` file is a minimal failing input committed in the same change as its fix. Two gates per fixture:

- `regression_inputs_preserve_html`: `format_validated` must succeed (HTML equivalent to source). Skipped for fixtures
  whose stem ends in `.idem`.
- `regression_inputs_are_idempotent`: byte equality across two format passes. Applied to every fixture.

**Invariant:** previously-broken shapes do not re-regress.

**Does NOT cover:** anything not in the file list. Adding a fixture is the way to lock in a new invariant.

## GFM spec snapshot

**Location:** `tests/gfm_spec.rs`, vendored spec at `tests/gfm-spec/spec.txt`, snapshot at
`tests/gfm-spec/snapshot.txt`.

Two tests:

- `gfm_spec_snapshot`: runs every spec case and compares the residual allowlist against `snapshot.txt`. Update with
  `MDWRIGHT_UPDATE_SNAPSHOT=1`.
- `gfm_spec_coverage`: asserts the bucketing (fully matching / intentional dev / tracked regression / unexpected) and
  refuses any `unexpected` count.

**Invariant:** the formatter's GFM conformance is stable; the snapshot only changes when intentionally rebaselined.

**Does NOT cover:** behaviour outside the GFM-spec cases. Project-specific extensions (admonitions, frontmatter, math
regions) live in their own golden suites.

## Parser backend audit

**Location:** `cargo xtask parser-audit`, classifications in `docs/architecture/parser-backend-audit.md`.

The audit compares mdwright's `pulldown-cmark` backend against the vendored cmark-gfm expected HTML and a pinned
`cmark-gfm` binary. It renders mdwright through the `cmark-gfm` render profile so parser drift is not hidden by HTML
serializer spelling. Optional comrak output is reported as diagnostic evidence, not as a release gate. The audit also
performs risk-gated source-position checks for constructs that mdwright uses as formatter or lint facts.

**Invariant:** parser-backend differences are explicit. Unclassified pulldown HTML mismatches, unclassified
source-position risks, uncontained parser panics, rows marked `fixed`, and rows marked `needs-mdwright-mitigation` fail
the command.

**Does NOT cover:** formatter idempotence or rewrite safety; those remain covered by the GFM snapshot, property tests,
fuzz, and production soak.

## Fuzz oracles

**Location:** `fuzz/fuzz_targets/`.

| Target | Oracle | Option byte |
|---|---|---|
| `fuzz_idempotence` | `format(format(s)) == format(s)` | Yes; drives wrap × mode × math × canonicalisation |
| `fuzz_parse_format` | `semantically_equivalent(s, format(s))` | Yes; same allocation as `fuzz_idempotence` |
| `fuzz_structured_idempotence` | Structured-document idempotence over generated Markdown | Yes |
| `fuzz_verbatim_identity` | Default options are identity modulo document-boundary normalisations | No |
| `fuzz_lint` | Standard lint rules do not panic and diagnostics are deterministic/in-bounds | No |
| `fuzz_latex_render` | TeX math-body parse plus Unicode render never panics; malformed or unsupported input returns typed errors | No |
| `fuzz_latex_translate` | LaTeX-to-Unicode and Unicode-to-LaTeX source translation never panic; diagnostic/loss spans stay in bounds | No |
| `fuzz_markdown_math_translate` | Markdown math-span scanning plus body-only translation never panics and preserves valid span accounting | No |

**Option byte allocation** (`fuzz_idempotence` and `fuzz_parse_format`, identical):

| Bits  | Field |
|-------|-------|
| 0–1   | `wrap` (Keep, No, At(80), At(120)) |
| 2     | `math.normalise` |
| 3     | reserved for corpus continuity |
| 4–7   | Canonicalisation mode (16 enumerated: preserve, one per style knob, two combined) |

**Invariant:** no input causes a panic or property violation in 10 minutes. Parser implementation panics are converted
to `ParseError` at the `mdwright-document` boundary, so fuzz targets discard parse errors through normal `Result`
handling rather than wrapping product calls in `catch_unwind`. TeX math-body failures return `LatexError` or translation
diagnostics through `mdwright-latex`; fuzz treats those as normal product output and checks that reported spans are
valid. Findings are committed to `crates/mdwright/tests/regressions/` or to `mdwright-latex` coverage fixtures as
appropriate.

## Production soak

`cargo xtask production-soak --corpus-root <path>` runs parser, lint, format-validation, idempotence, and fmt-check
checks over the corpus enumerated by `crates/mdwright/benches/corpus.list` plus representative external Markdown
fixtures. The command reports parse errors, validation failures, idempotence failures, fmt-check disagreements, rewrite
candidate totals, maximum file size, and slowest files.

**Does NOT cover:** behaviour beyond `MAX_INPUT = 65 536` bytes; the libFuzzer harness skips bigger inputs. The CLI
enforces the same shape via `--max-input-bytes`.

## mdformat parity

`cargo xtask mdformat-parity --corpus-root <path> --corpus-name <name> --mdwright-config <path> --mdformat-config
xtask/fixtures/mdformat-parity/mdformat.toml` copies a corpus into isolated temp roots, runs mdwright and mdformat, and
writes JSON / Markdown reports under `target/mdwright/parity/`. The command compares changed file sets, line-diff stats,
idempotence, mdBook buildability when applicable, and semantic equivalence of each formatter output to the original.

The mdformat config is checked in as an xtask fixture so mdformat does not look like the repository's own formatter.

The parity gate is intentionally not byte-equality with mdformat. Differences are allowed only when
`docs/architecture/mdformat-parity.md` classifies them as configured, intentional, or upstream-owned. The command fails
on unclassified differences, mdwright semantic drift, parser errors, idempotence failures, mdBook failures, rows marked
`fixed` that still appear, and rows marked `open-bug`.

## Release evidence

`cargo xtask release-evidence --output target/mdwright/release` aggregates local release-candidate evidence into
`release-evidence.json` and `release-evidence.md`. The command records git state and tool versions, reads existing
parser-audit, mdformat-parity, production-soak, and package/install reports, and points at manual notes for fast checks,
fuzz rounds, and benchmarks.

**Invariant:** the release candidate has one inspectable evidence bundle that states the current claim, lists accepted
divergences, and names missing evidence as blockers.

**Does NOT cover:** running expensive gates. The command summarizes evidence; it does not replace parser-audit,
mdformat-parity, production-soak, fuzzing, packaging, or Criterion.

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

Lint-rule coverage lives with each rule under `src/stdlib/*` and `tests/`; that's a parallel matrix and isn't summarised
here. CLI-surface tests live at `tests/cli_*.rs`. The diagnostic JSON v2 schema is gated by
`tests/diagnostic_json_v2.rs`.
