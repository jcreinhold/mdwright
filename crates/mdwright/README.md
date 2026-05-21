# mdwright

[![docs.rs](https://docs.rs/mdwright/badge.svg)](https://docs.rs/mdwright)

A Markdown linter and round-trip formatter for any Markdown project, distributed as the
`mdwright` command-line tool.

`mdwright fmt` is HTML-equivalent to its input: it refuses any rewrite that would change
the rendered DOM. On a 79-file corpus, `mdwright fmt-check` runs ≥ 50× faster than
`mdformat --check` (see the project's
[Performance page](https://jcreinhold.github.io/mdwright/reference/performance.html)).
Math regions (`\( … \)`, `\[ … \]`, `\begin{…} … \end{…}`, `$ … $`) pass through
verbatim, so the tool stays safe on technical writing too.

This crate ships the binary and the thin orchestration glue (`mdwright::run_with_rules`,
`mdwright::discover_markdown`). The reusable analysis surface lives in the sibling crates
(`mdwright-document`, `mdwright-format`, `mdwright-lint`, `mdwright-latex`,
`mdwright-math`, `mdwright-config`, `mdwright-lsp`); depend on those directly if you are
embedding mdwright rather than running it.

## Install

```bash
# One-line install (Linux x86_64, macOS aarch64). No Rust toolchain required.
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/jcreinhold/mdwright/releases/latest/download/mdwright-installer.sh | sh

# Or from crates.io (any target with a Rust toolchain).
cargo install mdwright

# Or via cargo-binstall.
cargo binstall mdwright
```

## Quick start

```bash
# CI idiom: lint + format-check, fail on any issue.
mdwright check --check . && mdwright fmt-check .

# Apply every safe autofix in place.
mdwright fix docs/

# Reformat (round-trip safe).
mdwright fmt docs/

# Read a single file from stdin.
cat note.md | mdwright check -
```

## Status

Pre-1.0. The CLI surface is stable enough to use in CI; breaking changes ship without
deprecation warnings.

## See also

- Project README and full feature pitch: <https://github.com/jcreinhold/mdwright>
- Manual: <https://jcreinhold.github.io/mdwright/>
- Rules catalogue: <https://jcreinhold.github.io/mdwright/rules/index.html>
- Configuration: <https://jcreinhold.github.io/mdwright/configuration.html>

## License

Licensed under MIT or Apache-2.0, at your option.
