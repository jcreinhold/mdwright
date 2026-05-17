# Math regions

This is mdwright's reason for existing. Generic Markdown formatters mangle LaTeX: they reflow
`\frac{a}{b}` into `\\frac{a}{b}`, collapse the blank line before `\begin{align*}`, and apply emph
rules inside `\(\alpha\)`. mdwright treats math as opaque — recognised before any other pass runs,
emitted verbatim.

## What counts as math

The default math grammar:

- **Inline.** `\( … \)` (paired backslash-paren, may span a single line).
- **Display.** `\[ … \]` (paired backslash-bracket, may span multiple lines).
- **Environments.** `\begin{NAME} … \end{NAME}` for any `NAME` matching `[A-Za-z][A-Za-z0-9*]*`,
  paired with non-overlapping `\end{NAME}`.

`$ … $` and `$$ … $$` are **not** math by default. Dollar-delimited math is common in academic
prose but collides with literal-dollar use (prices, shell prompts). Opt in via configuration:

```toml,no-check
[lint]
math.dollar = true
```

The [stray-dollar](../rules/stray-dollar.md) lint flags lone dollar signs when this option is off
so authors switching from a dollar-delimited dialect catch the migration cost.

## How the scanner runs first

The interesting design choice — explained in
[`src/format/math.rs`](https://github.com/jcreinhold/mdwright/blob/main/src/format/math.rs) — is
that the math scanner runs **before** the `pulldown-cmark` event walk that builds the formatter's
typed IR. The scanner produces a sorted list of byte ranges; the IR builder consults this list when
it would otherwise descend into a span and instead emits a `NodeKind::Math` leaf whose payload is
the verbatim source slice.

This means:

- Math is not "parsed" by mdwright. The exact source bytes — whitespace, casing, comment chars,
  trailing backslashes — pass through unchanged.
- The formatter cannot accidentally apply emphasis, escape, or wrap logic to math: those passes
  walk the IR tree, and math is a leaf they do not recurse into.
- Lint rules that match on text get the same opaque region: see
  [latex-command](../rules/latex-command.md), which only fires *outside* math regions.

## Block-level math

A math environment whose start delimiter is at column 1 of an otherwise-blank line is treated as a
block. The formatter emits blocks with one blank line above and below, never indented inside list
items unless the source already indented them. This avoids the common bug where a generic
formatter strips the blank line before `\begin{align*}` and breaks the rendered DOM.

## Math-adjacent rules

Three rules check math without parsing it:

- [math/unbalanced-delim](../rules/math/unbalanced-delim.md) — `\(` without `\)`.
- [math/unbalanced-env](../rules/math/unbalanced-env.md) — `\begin{x}` without matching `\end{x}`.
- [math/unbalanced-braces](../rules/math/unbalanced-braces.md) — `{` count diverges from `}` count
  inside a region.

Each runs on the recognised region as a string; none of them care about the math semantics.

## When math leaks

If a `\(` appears inside a fenced code block, mdwright does not treat it as math — code blocks are
recognised earlier still. If `\(` appears in inline code (`` `\(x\)` ``), the same exemption
applies. The math scanner is aware of escape and code regions so it does not produce false
positives inside them.

## See also

- [Round-trip safety](round-trip-safety.md) — the gate that catches math corruption.
- [Configuration](../configuration.md) — math options under `[fmt.math]` and `[lint.math]`.
- [stray-dollar rule](../rules/stray-dollar.md) — migration aid for dollar-delimited corpora.
