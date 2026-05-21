# mdwright-math

[![docs.rs](https://docs.rs/mdwright-math/badge.svg)](https://docs.rs/mdwright-math)

Markdown math-region recognition for [mdwright](https://github.com/jcreinhold/mdwright):
delimiter policy, region scanning, and balance diagnostics for TeX-style math embedded in
Markdown.

CommonMark does not understand math. Inside `\[ … \]`, `\( … \)`, `$$ … $$`, `$ … $`, or
`\begin{env} … \end{env}`, pulldown tokenises bytes as plain prose, so `_` becomes
emphasis, `[` becomes a link candidate, and `*` becomes a delimiter run. Without an overlay
the formatter's round-trip drifts. This crate is that overlay: `scan::scan_math_regions`
consumes Markdown source plus the IR's inline and block atoms and produces typed
`MathRegion` values plus the `math/unbalanced-delim`, `math/unbalanced-env`, and
`math/unbalanced-braces` diagnostics.

Math-body grammar (TeX commands, Unicode layout, source translation) is delegated to the
`mdwright-latex` crate. Pulldown invocation and document facts are owned by
`mdwright-document`.

## Status

Pre-1.0. Public items are whatever `lib.rs` re-exports; breaking changes ship without
deprecation warnings.

## Use it

```toml
[dependencies]
mdwright-math = "0.1"
```

```rust
use mdwright_math::scan;
```

## See also

- Project: <https://github.com/jcreinhold/mdwright>
- Library walkthrough: <https://jcreinhold.github.io/mdwright/reference/public-api.html#use-mdwright-as-a-library>
- Manual: <https://jcreinhold.github.io/mdwright/>
- Math rendering: <https://jcreinhold.github.io/mdwright/concepts/math-rendering.html>

## License

Licensed under MIT or Apache-2.0, at your option.
