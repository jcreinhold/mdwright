# GitHub Actions

Lint and format-check Markdown on every push and pull request.

## Minimal workflow

Save as `.github/workflows/markdown.yml`:

```yaml,no-check
name: markdown
on:
  push:
    branches: [main]
  pull_request:

jobs:
  mdwright:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install mdwright --locked
      - run: mdwright check --check .
      - run: mdwright fmt-check .
```

The two `cargo install` and `Swatinem/rust-cache@v2` steps make subsequent runs fast (~5 s
warm). `--locked` pins the version recorded in `Cargo.lock` once the project is added as a
dependency.

## With pre-built binaries

> Available once 0.2.0 ships prebuilt binaries via cargo-dist.

```yaml,no-check
      - uses: cargo-bins/cargo-binstall@main
      - run: cargo binstall --no-confirm mdwright
      - run: mdwright check --check .
```

This skips the compile step entirely — runs cold in under 10 seconds.

## Reading the output in PR annotations

mdwright's pretty output is human-readable in the Actions log. For PR annotations (squiggles in
the GitHub UI), pipe JSON v2 through a converter — there is no first-class action yet, but the
schema is documented at [Diagnostic schema](../reference/diagnostic-schema.md) and stable across
0.x.

## See also

- [Pre-commit](pre-commit.md) — client-side gate before push.
- [CI recipes](ci-recipes.md) — non-GitHub CI providers.
