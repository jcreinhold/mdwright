# mdwright-latex

TeX math-body parsing, Unicode terminal layout, and source translation for
[mdwright](https://github.com/jcreinhold/mdwright).

This crate owns the volatile language machinery on both sides of the math-body boundary:
a TeX-like lexer and parser, the command vocabulary, terminal-width layout for Unicode
math, and translation between Unicode mathematical source and LaTeX. It is the only place
in the workspace where math-body grammar lives.

Markdown delimiter recognition (`$ … $`, `\[ … \]`, `\begin{…} … \end{…}`) is not in scope —
that belongs to `mdwright-math`, which consumes this crate to analyse the bodies it scans
out. Command-line delivery belongs to the `mdwright` binary crate.

The public surface is intentionally narrow: lexer tokens and AST nodes stay private, and
callers interact through result and diagnostic types such as `RenderedLatex`,
`render_unicode_math`, and the registry of recognised commands.

## Status

Pre-1.0. Public items are whatever `lib.rs` re-exports; breaking changes ship without
deprecation warnings.

## Use it

```toml
[dependencies]
mdwright-latex = "0.1"
```

```rust
use mdwright_latex::render_unicode_math;
```

## See also

- Project: <https://github.com/jcreinhold/mdwright>
- Manual: <https://jcreinhold.github.io/mdwright/>
- Math rendering: <https://jcreinhold.github.io/mdwright/concepts/math-rendering.html>

## License

Licensed under MIT or Apache-2.0, at your option.
