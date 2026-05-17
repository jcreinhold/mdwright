# Installation

mdwright has no runtime dependencies. Pick whichever channel matches your environment.

## From source

```sh
cargo install mdwright
```

Requires Rust 1.91 or later (the MSRV is enforced in CI). The install drops a single binary, `mdwright`, on your
`$PATH`.

## Prebuilt binaries

> Ships in 0.2.0 alongside the first tagged crates.io release. See
> [issue tracker](https://github.com/jcreinhold/mdwright/issues) for the release schedule.

Once 0.2.0 lands:

- **`cargo binstall mdwright`** — pulls a prebuilt binary from the GitHub release matching your platform, falling back
  to `cargo install` if no binary is available.
- **Homebrew** — `brew install jcreinhold/tap/mdwright` (macOS / Linux).
- **Direct download** — pick a `.tar.gz` / `.zip` from the
  [GitHub releases page](https://github.com/jcreinhold/mdwright/releases).

## Building from a clone

```sh
git clone https://github.com/jcreinhold/mdwright
cd mdwright
cargo build --release
./target/release/mdwright --help
```

`cargo nextest run` exercises the full test suite (golden snapshots, GFM spec runner, property tests). `cargo bench`
runs the Criterion benches; `cargo xtask doc-rules --check` and `cargo xtask doc-cli --check` verify that the
auto-generated documentation pages are up to date.

## Platform support

| Tier | Targets                                                                      | Coverage                           |
| ---- | ---------------------------------------------------------------------------- | ---------------------------------- |
| 1    | `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc` | CI matrix on every push            |
| 2    | `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`                           | Best-effort; CI on tagged releases |

Other targets work in principle but are not tested.
