# mdwright

A math-resilient Markdown linter for Kan documentation. It flags the control-sequence patterns that generic Markdown
formatters routinely mangle in mathematical prose — `\_` escapes left behind by an emphasis pass, stray `$` from
would-be code spans, LaTeX commands in Unicode-mathematics documents, and damaged subscripts like `Hom*{cart}`.

It also includes a round-trip formatter (`fmt` / `fmt --check`) that is GFM-spec compliant, idempotent on the project
corpus, and ~580× faster than `mdformat` on the same workload (see [Performance](#performance)).

## Quick start

```bash
# Lint a tree, exit non-zero on any non-advisory diagnostic.
mdwright check --check docs/

# Apply every safe autofix in place.
mdwright fix docs/

# Compact, grep-friendly output.
mdwright check --format compact docs/ | grep stray-dollar

# Pipe a single file through stdin.
cat note.md | mdwright check
```

Pass files, directories, or both. Directories are walked recursively with `.gitignore` honoured. With no paths, the tool
reads stdin and reports the path as `<stdin>`.

## Rules

| Name                     | Catches                                                       | Advisory |
| ------------------------ | ------------------------------------------------------------- | -------- |
| `escaped-emphasis`       | `\_` / `\*` in prose — leftover italic-vs-subscript damage    | no       |
| `stray-dollar`           | `$` in prose; in this project it is always a typo for `` ` `` | no       |
| `latex-command`          | `\foo` control sequences (project convention is Unicode)      | no       |
| `subscript-damage`       | `Hom*{cart}`, `α*f` — a `_` flipped to `*` by mdformat        | no       |
| `adjacent-code-no-space` | `` `foo`bar `` — re-tokenised ambiguously by some renderers   | no       |
| `unbalanced-backtick`    | Unclosed inline code span                                     | no       |
| `unicodeable-subscript`  | `^{-1}`, `_{2}` that have Unicode equivalents                 | yes      |

Run `mdwright list-rules` to print the live catalogue. Advisory rules report findings but do not fail `--check`.

Select rules with `--only escaped-emphasis,stray-dollar` or exclude them with `--skip unicodeable-subscript`. The two
flags do not combine.

## Spec conformance

The formatter round-trips the GFM 0.29-gfm spec (vendored at
`tests/gfm-spec/spec.txt`). Current coverage:

| Bucket               | Examples |
| -------------------- | -------- |
| Spec examples total  | 672      |
| Matching             | 615      |
| Editorial deviations | 35       |
| Tracked regressions  | 22       |

See [docs/deviations.md](docs/deviations.md) for the per-section
breakdown and how the snapshot / allowlist mechanism works. The
`--mode=verbatim` flag bypasses the typed-IR path and preserves the
source byte-for-byte for documents where the default normalising mode
would diverge.

Live numbers (the table above is a release snapshot):

```sh
cargo test --release --test gfm_spec gfm_spec_coverage -- --nocapture
```

## Performance

`mdwright fmt --check` is **~580× faster than `mdformat --check`** on the Kan documentation corpus.

| Command                      | Mean ± σ     |
| ---------------------------- | ------------ |
| `mdwright check docs/`       | 147 ± 5 ms   |
| `mdwright fmt --check docs/` | 128 ± 4 ms   |
| `mdformat --check docs/`     | 74.6 ± 5.5 s |

Both tools parse the same Markdown and verify that a round-trip would be a no-op. mdformat is single-threaded Python;
mdwright is Rust and walks files in parallel with rayon, so most of the gap is the platform, not the algorithm.

The v0.3.0 spec-alignment redesign moved each CM/GFM construct into a typed IR value with its own `pretty()` method.
For the steady-state format step (parse outside the timed region), this is **25–27 % faster** than the v0.2.0 sieve
on the project's micro-benches: `format/small` 0.216 ms → 0.159 ms, `format/medium` 0.368 ms → 0.271 ms. The end-to-end
parse-plus-format path is 8–15 % slower because IR construction now does more work per event; the headline
`mdwright fmt --check docs/` number above is dominated by parallel I/O and parse, so the regression is not visible at
that level.

**Experimental conditions.** Apple M4 Pro (12 cores), 24 GB RAM, macOS 26.4.1. Mdwright built with `rustc 1.95.0` on the
`release` profile, default `RUSTFLAGS`. Mdformat 1.0.0 with the `mdformat-gfm`, `mdformat_footnote`, `mdformat_mkdocs`,
and `mdformat_frontmatter` plugins, installed via `uv`. Corpus: 2,107 Markdown files under `docs/`, ≈ 620k lines, ≈ 35
MB. Measurement: `hyperfine 1.20.0`, three warm-up runs and ten timed runs per command, all three commands in one
invocation so system-load drift affects them equally.

Numbers vary by hardware and corpus shape; re-measure before quoting.

## Output formats

- `pretty` (default): coloured when stdout is a tty; one block per file.
- `compact`: `path:line:col: rule: message`, one per line.
- `json`: JSON Lines, one object per diagnostic.

## Exit codes

| Code | Meaning                                                          |
| ---- | ---------------------------------------------------------------- |
| 0    | Success. With `--check`: no non-advisory diagnostics.            |
| 1    | `--check` and at least one non-advisory diagnostic was reported. |
| 2    | I/O, argument, or other operational error (details on stderr).   |

## Debugging

`mdwright check`, `fmt`, etc. accept `-v` (repeated to increase verbosity):

| Flag   | Level | Use case                                  |
| ------ | ----- | ----------------------------------------- |
| (none) | warn  | normal operation (silent on success)      |
| `-v`   | info  | high-level pipeline stages                |
| `-vv`  | debug | per-block / per-construct decisions       |
| `-vvv` | trace | per-byte escape decisions, delimiter runs |

`RUST_LOG` overrides the flag when set, e.g.
`RUST_LOG=mdwright::format::inline=trace mdwright fmt foo.md`. The default filter is scoped to the `mdwright` crate, so
transitive dependencies stay quiet at every level.

## Safety

`mdwright` is built to be safe on untrusted input — pipelines that
feed it `.md` files from contributors, web forms, or third-party
repositories should not be able to crash it, exhaust memory, or hang
it.

The CLI imposes four bounds:

| Bound                | Default       | Override                  |
| -------------------- | ------------- | ------------------------- |
| Single-file size cap | 10 MB         | `--max-input-bytes BYTES` |
| Symlink following    | off (no loop) | (compile-time)            |
| Paragraph token cap  | 100 000 boxes | (compile-time)            |
| Wrap-DP time budget  | 100 ms        | (compile-time)            |

When a bound trips the formatter degrades gracefully — paragraphs
past the token / time cap are emitted without re-wrapping, and files
past the size cap are refused with a clear non-zero exit. The library
(`Document::parse`) imposes no size cap; callers feeding untrusted
bytes from non-CLI front-ends are responsible for bounding input
themselves.

Five coverage-guided fuzz targets live under [`fuzz/`](./fuzz):

- `fuzz_parse_format` — `html(s) == html(format(s))`; format must
  not change the rendered HTML (mirrors the `format_validated` CLI
  gate).
- `fuzz_idempotence` — `format(parse(format(parse(s))))` ≡
  `format(parse(s))`. First input byte drives `FmtOptions` (wrap ×
  mode × `math.normalise`).
- `fuzz_lint` — every standard-library lint rule on every input,
  must not panic; every diagnostic span lies in `0..len`; lint
  output is deterministic.
- `fuzz_verbatim_identity` — verbatim mode is idempotent and, when
  the source is in canonical boundary form, a strict identity.
- `fuzz_structured_idempotence` — `arbitrary`-driven block-template
  generator builds Markdown documents biased toward shapes that
  have historically surfaced idempotence bugs; same oracle as
  `fuzz_idempotence` with the option byte too.

Run a target with `cd fuzz && cargo +nightly fuzz run <target> --
-max_total_time=300 -dict=dict/markdown.dict`. The dictionary holds
~60 high-value CommonMark / GFM / math tokens and cuts
time-to-first-coverage on rare constructs (math envs, admonitions,
tables, frontmatter).

Reproducer inputs that surface real bugs are checked in under
[`tests/regressions/fuzz_*.in`](./tests/regressions) and gated by the
suite in `tests/regressions.rs`. Open bugs (fix deferred) live under
[`fuzz/known-issues/`](./fuzz/known-issues) and are pinned by
`tests/known_issues.rs` so silent drift fails CI even while the bug
itself stays in.

**Corpus hygiene.** Before committing corpus changes, minimise:

```sh
cd fuzz
for t in fuzz_parse_format fuzz_idempotence fuzz_lint \
         fuzz_verbatim_identity fuzz_structured_idempotence; do
  cargo +nightly fuzz cmin "$t"
done
```

Target ≤2k entries per corpus post-minimisation. Larger corpora
slow iter/sec without buying coverage.

Reports of panics on any input are security bugs; see
[SECURITY.md](./SECURITY.md) for disclosure.

## Library

`mdwright` is also a Rust library. The surface is small:

```rust
use mdwright::{Document, RuleSet};

let doc = Document::parse(source);
let diags = doc.lint(&RuleSet::all());
let (fixed, n) = Document::apply_safe_fixes(source, &diags);
```

`Document` is parsed once and may be linted repeatedly with different rule sets. `apply_safe_fixes` ignores diagnostics
whose fix is unsafe and resolves overlapping edits right-to-left.

## Building

```bash
cargo build --release
cargo test
cargo clippy --all-features --all-targets -- -D warnings
```

The crate sits outside the main Cargo workspace by design; build it from this directory.
