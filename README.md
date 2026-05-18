# mdwright

[![ci](https://github.com/jcreinhold/mdwright/actions/workflows/ci.yml/badge.svg)](https://github.com/jcreinhold/mdwright/actions/workflows/ci.yml)
[![docs](https://github.com/jcreinhold/mdwright/actions/workflows/docs.yml/badge.svg)](https://jcreinhold.github.io/mdwright/)

A math-resilient Markdown linter and round-trip formatter for Kan documentation. It flags the
control-sequence patterns that generic Markdown formatters routinely mangle in mathematical prose,
and `mdwright fmt` is HTML-equivalent to its input — the formatter refuses any rewrite that would
change the rendered DOM.

**Preserve-by-default.** `mdwright fmt` keeps your source's style choices — emphasis delimiters
(`_foo_` vs `*foo*`), list markers (`-` / `*` / `+`), thematic breaks, link-destination angle
brackets — untouched. When you want consistent style across a project, opt in per-knob via
`.mdwright.toml`; see [Formatter policy](https://jcreinhold.github.io/mdwright/format/policy.html).
If you want aggressive cross-knob canonicalisation as the default, [mdformat](https://mdformat.readthedocs.io/)
is a good alternative.

It is also fast: ~580× faster than `mdformat --check` on a 2,107-file corpus.

## Documentation

Full manual: **<https://jcreinhold.github.io/mdwright/>**

- [Getting started](https://jcreinhold.github.io/mdwright/getting-started.html) — ten-minute
  walkthrough.
- [Rules catalogue](https://jcreinhold.github.io/mdwright/rules/index.html) — every shipped rule
  with examples.
- [Configuration](https://jcreinhold.github.io/mdwright/configuration.html) — `.mdwright.toml`
  schema.
- [Architecture](https://jcreinhold.github.io/mdwright/extending/architecture.html) — the two-IR
  design.
- [Integration](https://jcreinhold.github.io/mdwright/integration/pre-commit.html) — pre-commit,
  GitHub Actions, editor flows.

## Quick start

```bash
# Lint a tree, exit non-zero on any non-advisory diagnostic.
mdwright check --check docs/

# Apply every safe autofix in place.
mdwright fix docs/

# Reformat (round-trip safe).
mdwright fmt docs/

# Compact, grep-friendly output.
mdwright check --format compact docs/ | grep stray-dollar

# Pipe a single file through stdin.
cat note.md | mdwright check
```

Pass files, directories, or both. Directories are walked recursively with `.gitignore` honoured.
With no paths, the tool reads stdin and reports the path as `<stdin>`.

`mdwright explain <rule>` prints the long-form rationale of any rule, plus a link into the doc
site.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| 0    | Success. With `--check`: no non-advisory diagnostics. |
| 1    | `--check` and at least one non-advisory diagnostic was reported. |
| 2    | I/O, argument, or other operational error (details on stderr). |

## Safety

`mdwright` is built to be safe on untrusted input — pipelines that feed it `.md` files from
contributors, web forms, or third-party repositories should not be able to crash it, exhaust
memory, or hang it.

The CLI imposes four bounds:

| Bound | Default | Override |
| ----- | ------- | -------- |
| Single-file size cap | 10 MB | `--max-input-bytes BYTES` |
| Symlink following | off (no loop) | (compile-time) |
| Paragraph token cap | 100 000 boxes | (compile-time) |
| Wrap-DP time budget | 100 ms | (compile-time) |

When a bound trips the formatter degrades gracefully — paragraphs past the token / time cap are
emitted without re-wrapping, and files past the size cap are refused with a clear non-zero exit.
The library (`Document::parse`) imposes no size cap; callers feeding untrusted bytes from non-CLI
front-ends are responsible for bounding input themselves.

Five coverage-guided fuzz targets live under [`fuzz/`](./fuzz):

- `fuzz_parse_format` — `html(s) == html(format(s))`; format must not change the rendered HTML
  (mirrors the `format_validated` CLI gate). First input byte drives the same option matrix as
  `fuzz_idempotence`.
- `fuzz_idempotence` — `format(parse(format(parse(s))))` ≡ `format(parse(s))`. First input byte
  drives `FmtOptions` (wrap × mode × `math.normalise` × canonicalisation mode).
- `fuzz_lint` — every standard-library lint rule on every input, must not panic; every diagnostic
  span lies in `0..len`; lint output is deterministic.
- `fuzz_verbatim_identity` — verbatim mode is idempotent and, when the source is in canonical
  boundary form, a strict identity.
- `fuzz_structured_idempotence` — `arbitrary`-driven block-template generator builds Markdown
  documents biased toward shapes that have historically surfaced idempotence bugs; same oracle as
  `fuzz_idempotence` with the option byte too.

Run a target with
`cd fuzz && cargo +nightly fuzz run <target> -- -max_total_time=300 -dict=dict/markdown.dict`.
The dictionary holds ~60 high-value CommonMark / GFM / math tokens and cuts time-to-first-coverage
on rare constructs.

Reproducer inputs that surface real bugs are checked in under
[`tests/regressions/fuzz_*.in`](./tests/regressions) and gated by the suite in
`tests/regressions.rs`. Open bugs (fix deferred) live under
[`fuzz/known-issues/`](./fuzz/known-issues) and are pinned by `tests/known_issues.rs` so silent
drift fails CI even while the bug itself stays in.

Reports of panics on any input are security bugs; see [SECURITY.md](./SECURITY.md) for disclosure.

## Library

`mdwright` is also a Rust library. The surface is small:

```rust
use mdwright::{Document, RuleSet};

let doc = Document::parse(source);
let diags = doc.lint(&RuleSet::all());
let (fixed, n) = Document::apply_safe_fixes(source, &diags);
```

`Document` is parsed once and may be linted repeatedly with different rule sets.
`apply_safe_fixes` ignores diagnostics whose fix is unsafe and resolves overlapping edits
right-to-left. Writing your own rules: see
[Extending → Lint rules](https://jcreinhold.github.io/mdwright/extending/lint-rules.html).

## Building

```bash
cargo build --release
cargo nextest run
cargo clippy --all-targets -- -D warnings
```

mdwright requires Rust ≥ 1.91 (declared in `Cargo.toml` as `rust-version`).

## Platform support

mdwright is tested on Linux, macOS, and Windows against both stable Rust and the MSRV (1.91) on
every push and pull request. See [`.github/workflows/ci.yml`](.github/workflows/ci.yml) for the
matrix and [`CONTRIBUTING.md`](CONTRIBUTING.md) for the MSRV-bump policy.
