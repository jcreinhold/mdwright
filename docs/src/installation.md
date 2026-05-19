# Installation

mdwright has no runtime dependencies — it ships as a single binary. Pick whichever channel matches your environment.

## One-line install (recommended)

No Rust toolchain required. The cargo-dist shell installer pulls the prebuilt binary for your platform from the latest
release and places it on your `$PATH`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/jcreinhold/mdwright/releases/latest/download/mdwright-installer.sh | sh
```

Supported targets: Linux x86_64, macOS aarch64 (see [Platform support](#platform-support) below).

## From crates.io

```sh
cargo install mdwright
```

Requires Rust 1.91 or later (the MSRV is enforced in CI). The install drops a single binary, `mdwright`, on your
`$PATH`. Rust integrations depend on the component crates directly — see [Public API surface](reference/public-api.md)
for the roster and what each owns.

## Via cargo-binstall

`cargo-binstall` pulls the GitHub-release tarball for your target and falls back to a source build if no prebuilt binary
is available:

```sh
cargo binstall mdwright
```

## Release tarball

Download a `.tar.xz` directly from the [GitHub releases page](https://github.com/jcreinhold/mdwright/releases) and
place the `mdwright` binary on your `$PATH`. Useful for air-gapped environments or when you want to pin a specific build
artifact.

## Building from a clone

```sh
git clone https://github.com/jcreinhold/mdwright
cd mdwright
cargo build --release -p mdwright
./target/release/mdwright --help
```

`cargo nextest run` exercises the full test suite (golden snapshots, GFM spec runner, property tests). `cargo bench`
runs the Criterion benches; `cargo xtask doc-rules --check` and `cargo
xtask doc-cli --check` verify that the
auto-generated documentation pages are up to date.

## Platform support

| Tier | Targets                                              | Coverage                                                  |
| ---- | ---------------------------------------------------- | --------------------------------------------------------- |
| 1    | `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`   | CI on every push; prebuilt binary attached to each release |
| 2    | `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu` | CI on every push; source build via `cargo install`        |

Other targets work in principle but are not tested.
