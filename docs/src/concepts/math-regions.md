# Math regions

This is mdwright's reason for existing. Generic Markdown formatters mangle LaTeX: they reflow
`\frac{a}{b}` into `\\frac{a}{b}`, collapse the blank line before `\begin{align*}`, and apply
emphasis rules inside `\(\alpha\)`. mdwright treats math as opaque — recognised before any other
pass runs, emitted verbatim.

## What counts as math

The default math grammar:

- **Inline.** `\( … \)` (paired backslash-paren, single line).
- **Display.** `\[ … \]` (paired backslash-bracket, may span lines).
- **Environments.** `\begin{NAME} … \end{NAME}` for any `NAME` matching `[A-Za-z][A-Za-z0-9*]*`,
  paired with a non-overlapping `\end{NAME}`.

`$ … $` and `$$ … $$` are **not** math by default. Dollar-delimited math is common in academic
prose but collides with literal-dollar use (prices, shell prompts). Opt in via configuration:

```toml,no-check
[lint]
math.dollar = true
```

The [stray-dollar](../rules/stray-dollar.md) lint flags lone dollar signs when this option is off,
so authors migrating from a dollar-delimited dialect catch the change.

## How the scanner runs

The math crate recognises candidate math spans over strings and byte ranges. The document crate
supplies Markdown exclusion ranges (code, HTML, other opaque regions), then stores the accepted
math regions as document facts with stable coordinates back to the original source. The exact
source bytes — whitespace, casing, comment chars, trailing backslashes — pass through unchanged.
The formatter cannot accidentally apply emphasis, escape, or wrap logic inside a math region:
rewrite candidates are verified against the document's math-region signature before they commit.
Lint rules that match on text see the same opaque region — [latex-command](../rules/latex-command.md),
for instance, only fires *outside* math.

## Block-level math

A math environment whose start delimiter sits at column 1 of an otherwise-blank line is a block.
The formatter emits blocks with one blank line above and below — never indented inside a list item
unless the source already indented it. This avoids the canonical bug:

```text
input:                          generic formatter:              mdwright:
A paragraph.                    A paragraph.                    A paragraph.
                                \begin{align*}                  
\begin{align*}                  E &= mc^2                       \begin{align*}
E &= mc^2                       \end{align*}                    E &= mc^2
\end{align*}                                                    \end{align*}
```

Stripping the blank line above `\begin{align*}` rolls the environment into the paragraph and
breaks the rendered DOM.

## Math-adjacent rules

Three rules check math without parsing it:

- [math/unbalanced-delim](../rules/math/unbalanced-delim.md) — `\(` without `\)`.
- [math/unbalanced-env](../rules/math/unbalanced-env.md) — `\begin{x}` without matching `\end{x}`.
- [math/unbalanced-braces](../rules/math/unbalanced-braces.md) — `{` count diverges from `}` count
  inside a region.

Each runs on the recognised region as a string; none of them care about the math semantics.

## Math inside code blocks

If `\(` appears inside a fenced code block or inline code (`` `\(x\)` ``), mdwright does not treat
it as math — code regions are recognised earlier still. The math scanner consults the same
exclusion ranges the formatter does, so it never produces false positives inside code or HTML.

## See also

- [Round-trip safety](round-trip-safety.md) — the gate that catches math corruption.
- [Configuration](../configuration.md) — math options under `[fmt.math]` and `[lint.math]`.
- [stray-dollar rule](../rules/stray-dollar.md) — migration aid for dollar-delimited corpora.
