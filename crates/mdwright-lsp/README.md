# mdwright-lsp

Language Server Protocol delivery for
[mdwright](https://github.com/jcreinhold/mdwright).

A tower-lsp server over stdio that exposes mdwright's lint diagnostics and formatter to
any LSP-aware editor. Diagnostics are debounced (~300 ms); formatting requires UTF-8
position encoding. The server is normally launched through the `mdwright lsp` subcommand,
which is what editor recipes invoke.

The diagnostic and formatting logic itself lives in `mdwright-lint` and `mdwright-format`;
this crate is the protocol shell.

## Status

Pre-1.0. Public items are whatever `lib.rs` re-exports; breaking changes ship without
deprecation warnings.

## Use it

Run the server through the binary:

```bash
mdwright lsp
```

Embed it directly:

```toml
[dependencies]
mdwright-lsp = "0.1"
```

```rust
use mdwright_lsp::serve;
```

## See also

- Project: <https://github.com/jcreinhold/mdwright>
- Editor integrations:
  <https://jcreinhold.github.io/mdwright/integration/editor-integrations.html>

## License

Licensed under MIT or Apache-2.0, at your option.
