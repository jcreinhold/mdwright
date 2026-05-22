# mdwright

[![ci](https://github.com/jcreinhold/mdwright/actions/workflows/ci.yml/badge.svg)](https://github.com/jcreinhold/mdwright/actions/workflows/ci.yml)
[![docs](https://github.com/jcreinhold/mdwright/actions/workflows/docs.yml/badge.svg)](https://jcreinhold.github.io/mdwright/)

A Markdown linter and round-trip formatter for any Markdown project. `mdwright fmt` is HTML-equivalent to its input: it
refuses any rewrite that would change the rendered DOM.

**Fast.** On a 79-file corpus of math-heavy technical prose, `mdwright fmt-check` runs ≥ 50× faster than
`mdformat --check`. Multipliers scale with file count and core count; see
[Performance](https://jcreinhold.github.io/mdwright/reference/performance.html) for the measurement and host details.

**Configurable, preserve by default.** Source style choices — emphasis delimiters (`_foo_` vs `*foo*`), list markers
(`-` / `*` / `+`), thematic breaks, link-destination angle brackets — pass through untouched. Canonicalisation is opt-in
one knob at a time in `.mdwright.toml`, or via `fmt.profile = "mdformat"` for mdformat-compatible spelling where
verified rewrites preserve the parsed document.

**Math-resilient.** Math regions (`\( … \)`, `\[ … \]`, `\begin{…} … \end{…}`, `$ … $`) pass through verbatim, so the
tool stays safe on technical writing too. The lint catalogue also covers control-sequence patterns that generic Markdown
formatters routinely mangle.

## Documentation

Full manual: **<https://jcreinhold.github.io/mdwright/>**

- [Getting started](https://jcreinhold.github.io/mdwright/getting-started.html)
- [Rules catalogue](https://jcreinhold.github.io/mdwright/rules/index.html)
- [Configuration](https://jcreinhold.github.io/mdwright/configuration.html)
- [Math rendering](https://jcreinhold.github.io/mdwright/concepts/math-rendering.html)
- [Integration](https://jcreinhold.github.io/mdwright/integration/pre-commit.html)

## Installation

```bash
# One-line install (Linux x86_64, macOS aarch64). No Rust toolchain required.
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/jcreinhold/mdwright/releases/latest/download/mdwright-installer.sh | sh

# Or from crates.io (any target with a Rust toolchain).
cargo install mdwright

# Or via cargo-binstall (Rust + binstall).
cargo binstall mdwright
```

Tarballs for each release are attached to the [GitHub Releases page](https://github.com/jcreinhold/mdwright/releases).
See [Installation](https://jcreinhold.github.io/mdwright/installation.html) for the full platform support matrix.

## Quick start

```bash
# CI idiom: lint + format-check together, exit non-zero on any failure.
mdwright check --check . && mdwright fmt-check .

# Lint a tree (exits 0 even with diagnostics; add --check to fail).
mdwright check docs/

# Apply every safe autofix in place.
mdwright fix docs/

# Reformat (round-trip safe).
mdwright fmt docs/

# Read a single file from stdin.
cat note.md | mdwright check -
```

| Subcommand | Writes | Exit non-zero when |
| --- | --- | --- |
| `mdwright check` | nothing | `--check` is set and a non-advisory diagnostic fires |
| `mdwright fix` | files (safe fixes) | `--check` is set and a non-advisory diagnostic still remains |
| `mdwright fmt` | files (every input) | parse fails or the safety gate refuses the rewrite |
| `mdwright fmt-check` | nothing | any input would be reformatted |

See [Lint vs. format](https://jcreinhold.github.io/mdwright/concepts/lint-vs-format.html) for when each fires.

A diagnostic looks like:

```text
error[bare-url]: bare URL should be wrapped in angle brackets or rendered as a link
  --> README.md:3:5
   |
 3 | See https://example.com for the spec.
   |     ^^^^^^^^^^^^^^^^^^^
   = help: CommonMark autolinks need angle brackets (`<https://example.com>`) to render as a link.
   = fix (safe): <https://example.com>
   = note: see `mdwright explain bare-url`
```

Every rule has a long-form explanation: `mdwright explain <rule>` prints the rationale and a link to the rendered rule
page.

Pass files, directories, or both; directories are walked recursively with `.gitignore` honoured. `mdwright lsp` runs a
built-in language server over stdio; recipes for Helix, Zed, VS Code, and Neovim are at
[Editor integration](https://jcreinhold.github.io/mdwright/integration/editor-integrations.html).

## Configure

mdwright walks up from `$PWD` to find a `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml [tool.mdwright]`. Out of
the box, no config file is needed: defaults preserve your source. A minimal `.mdwright.toml` looks like:

```toml
[lint]
# `default` enables the curated baseline; `ignore` removes rules.
preset = "default"
ignore = ["bare-url"]

[fmt]
list-marker = "dash"          # canonicalise list markers to `-`
```

See [Configuration](https://jcreinhold.github.io/mdwright/configuration.html) for the full schema.

## Wire into an existing project

`pre-commit`:

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jcreinhold/mdwright
    rev: v0.1.1
    hooks:
      - id: mdwright-check-system
      - id: mdwright-fmt-check-system
```

GitHub Actions:

```yaml
- uses: jcreinhold/mdwright@v0.1.1
  with:
    args: check --check .
```

Full recipes (including the `language: rust` variants that don't need `mdwright` on `$PATH`): see
[Pre-commit](https://jcreinhold.github.io/mdwright/integration/pre-commit.html) and
[GitHub Actions](https://jcreinhold.github.io/mdwright/integration/github-actions.html).

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success. With `--check`: no non-advisory diagnostics. |
| 1 | `--check` and at least one non-advisory diagnostic was reported. |
| 2 | I/O, argument, or other operational error (details on stderr). |

## Safety

`mdwright` is built to run on untrusted Markdown. The only user-tunable bound is `--max-input-bytes` (default 10 MB);
three further bounds are fixed at compile time and degrade gracefully. Paragraphs that overrun the token cap or 100 ms
wrap budget are emitted without re-wrapping rather than failing the run. Five coverage-guided fuzz targets under
[`fuzz/`](./fuzz) cover parse/format HTML equivalence, idempotence, lint determinism, and verbatim identity; reproducers
for fixed bugs live under [`crates/mdwright/tests/regressions/`](./crates/mdwright/tests/regressions). Panics on any
input are security bugs; see [SECURITY.md](./SECURITY.md) for disclosure.

## Compared to mdformat

mdwright is not a drop-in mdformat clone. The release gate includes `cargo xtask mdformat-parity`, which compares both
formatters on isolated corpus copies and records every byte-output difference as fixed, configured, intentional, or
upstream-owned in [`docs/architecture/mdformat-parity.md`](docs/architecture/mdformat-parity.md).
`[fmt] profile = "mdformat"` opts into mdformat-compatible spelling where verified rewrites preserve the parsed
document.

## Platform support

CI runs Linux × macOS × Windows against stable Rust and the 1.91 MSRV on every push and pull request. See
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) for the matrix.

## For contributors

Building from a clone, the Rust library crates, and writing custom lint rules are documented on the doc site:

- [Building](https://jcreinhold.github.io/mdwright/installation.html#building-from-a-clone)
- [Public API surface](https://jcreinhold.github.io/mdwright/reference/public-api.html)
- [Extending → Lint rules](https://jcreinhold.github.io/mdwright/extending/lint-rules.html)
- [Architecture](https://jcreinhold.github.io/mdwright/extending/architecture.html)
- [`CONTRIBUTING.md`](CONTRIBUTING.md) for the MSRV-bump policy and PR conventions.
