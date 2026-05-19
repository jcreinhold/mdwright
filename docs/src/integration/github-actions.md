# GitHub Actions

Lint and format-check Markdown on every push and pull request.

## Quickest path: the bundled composite action

mdwright publishes a composite action at the repo root (`action.yml`). It fetches the prebuilt
binary from the matching GitHub release and runs whatever `mdwright` command you pass:

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
      - uses: jcreinhold/mdwright@v0.1.0
        with:
          args: check --check .
      - uses: jcreinhold/mdwright@v0.1.0
        with:
          args: fmt-check .
```

`args` defaults to `check --check .`. Pin the version to a tag (`@v0.1.0`) rather than `@main` so
upstream releases don't silently rebreak your CI.

The action ships prebuilt binaries for `ubuntu-latest` (`x86_64-unknown-linux-gnu`) and
`macos-latest` (`aarch64-apple-darwin`). Other targets fall back to the source-build recipe below.

## Source-build fallback

For Windows runners or any platform we don't ship a prebuilt for, install from source:

```yaml,no-check
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

The `Swatinem/rust-cache@v2` step keeps subsequent runs around 5 s once the cache is warm; cold
builds take a couple of minutes.

## `cargo-binstall`

If you want the binary speed of the composite action without depending on the action's `action.yml`
contract, run `cargo-binstall` directly:

```yaml,no-check
      - uses: cargo-bins/cargo-binstall@main
      - run: cargo binstall --no-confirm mdwright
      - run: mdwright check --check .
```

This resolves the same release artifacts and skips the compile step.

## Reading the output in PR annotations

mdwright's pretty output is human-readable in the Actions log. For PR annotations (squiggles in the
GitHub UI), pipe JSON v2 through a converter; there is no first-class action yet, but the schema
is documented at [Diagnostic schema](../reference/diagnostic-schema.md) and stable across 0.x.

## See also

- [Pre-commit](pre-commit.md): client-side gate before push.
- [CI recipes](ci-recipes.md): non-GitHub CI providers.
