# mdwright-config

[![docs.rs](https://docs.rs/mdwright-config/badge.svg)](https://docs.rs/mdwright-config)

Configuration discovery and TOML resolution for
[mdwright](https://github.com/jcreinhold/mdwright).

`Config::load_explicit` reads a named file; `Config::discover` walks parents from a working
directory, honouring (in order) `.mdwright.toml`, `mdwright.toml`, and
`pyproject.toml [tool.mdwright]`, and stopping at a `.git/` boundary. The resolved config
hands typed parse, format, and lint policy to the other crates.

This crate is policy resolution only — it does not run rules, format files, or parse
Markdown. The TOML schema is documented in `docs/configuration.md`, generated from this
crate at build time.

## Status

Pre-1.0. Public items are whatever `lib.rs` re-exports; breaking changes ship without
deprecation warnings.

## Use it

```toml
[dependencies]
mdwright-config = "0.1"
```

```rust
use mdwright_config::Config;
```

## See also

- Project: <https://github.com/jcreinhold/mdwright>
- Library walkthrough: <https://jcreinhold.github.io/mdwright/reference/public-api.html#use-mdwright-as-a-library>
- Configuration reference: <https://jcreinhold.github.io/mdwright/configuration.html>

## License

Licensed under MIT or Apache-2.0, at your option.
