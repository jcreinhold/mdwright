## What it does

Flags TeX `\begin{env}` blocks that have no matching `\end{env}` (or vice versa), where `env` is
one of the math environments mdwright tracks (`equation`, `align`, `aligned`, `cases`, `matrix`,
`pmatrix`, `bmatrix`, `vmatrix`, `Vmatrix`, `gather`, `multline`, `split`, and their starred
variants).

## Why

`\begin{align} … \end{align}` and friends are math regions. Like `\[ … \]`, they suspend prose
lint rules and are passed through the formatter verbatim. An unmatched `\begin` leaves the
parser unable to tell where math ends; an unmatched `\end` is almost always a copy-paste error
that will silently break rendering in any math-aware renderer.

## Example (bad)

```markdown
\begin{align}
  a + b &= c \\
  d - e &= f
```

## Example (good)

```markdown
\begin{align}
  a + b &= c \\
  d - e &= f
\end{align}
```

## Configuration

- Disable inline: `<!-- mdwright: allow math/unbalanced-env -->`.
- Disable in config: `[lint] ignore = ["math/unbalanced-env"]`.
- Severity: non-advisory.

## References

- mdwright's environment recogniser (`src/stdlib/math_unbalanced_env.rs`).
- amsmath user's guide for the canonical environment list.
