# mdwright-format

Verified Markdown formatting and byte-rewrite transactions for
[mdwright](https://github.com/jcreinhold/mdwright).

The formatter is identity by default: structural emit replays the source. Active rewrites
ship as small, ownership-checked families — canonicalisation knobs (heading style, list
markers, link-definition shape, …) and paragraph wrapping. Each family produces its
normal-form plan or skips; rewrites that would change the rendered DOM are rejected by the
semantic gate (`semantically_equivalent`), so `format_validated` either returns a
byte-for-byte safe rewrite or surfaces a diagnostic.

Document facts come from `mdwright-document`; this crate does not parse Markdown itself.
Lint rules and CLI policy live in `mdwright-lint` and the `mdwright` binary crate.

## Status

Pre-1.0. Public items are whatever `lib.rs` re-exports; breaking changes ship without
deprecation warnings.

## Use it

```toml
[dependencies]
mdwright-format = "0.1"
```

```rust
use mdwright_format::{format_validated, FmtOptions};
```

## See also

- Project: <https://github.com/jcreinhold/mdwright>
- Manual: <https://jcreinhold.github.io/mdwright/>
- Architecture: `docs/architecture/crate-boundaries.md`

## License

Licensed under MIT or Apache-2.0, at your option.
