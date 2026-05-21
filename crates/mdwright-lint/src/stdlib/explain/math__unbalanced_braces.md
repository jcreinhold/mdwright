## What it does

Inside a math region, flags `{` and `}` whose depths do not balance: either an extra opening
brace with no matching close, or a stray `}`.

## Why

Math renderers (KaTeX, MathJax, pdflatex) all reject unbalanced braces, but they fail with
opaque messages far from the source location. Catching the imbalance in the linter pinpoints
the offending region in the markdown source, before the math reaches the renderer.

The check only runs inside math regions identified by [`math/unbalanced-delim`] and
[`math/unbalanced-env`]; balanced-brace checking in prose would be noise, since prose `{` and
`}` are not paired.

## Example (bad)

```markdown
$$\sum_{i=1^n i$$
```

## Example (good)

```markdown
$$\sum_{i=1}^n i$$
```

## Configuration

- Disable inline: `<!-- mdwright: allow math/unbalanced-braces -->`.
- Disable in config: `[lint] ignore = ["math/unbalanced-braces"]`.
- Severity: non-advisory.

## References

- mdwright's brace scanner (`src/stdlib/math_unbalanced_braces.rs`).
- TeX: braces group arguments to commands such as `\frac{a}{b}` and `\sum_{i}`.
