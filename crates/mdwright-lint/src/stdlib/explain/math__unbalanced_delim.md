## What it does

Flags display-math openers (`\[`) and inline-math openers (`\(`) that have no matching
closer (`\]` / `\)`) before the end of the document.

## Why

`\[ … \]` and `\( … \)` are TeX-style math delimiters. mdwright treats the region between an
opener and its closer as math: it suspends prose lint rules inside, and the formatter passes the
bytes through verbatim. An unbalanced opener means we cannot tell where math ends—every
following prose rule misreads the rest of the document, and the formatter might break the
content's rendering.

The check runs before any prose rule fires, so this is the first diagnostic you should fix in a
document.

## Example (bad)

```markdown
The Laplacian is \[ \Delta f = \sum_i \partial_i^2 f
```

## Example (good)

```markdown
The Laplacian is \[ \Delta f = \sum_i \partial_i^2 f \].
```

If you wanted a literal `\[` in prose, escape it: `\\[`.

## Configuration

- Disable inline: `<!-- mdwright: allow math/unbalanced-delim -->`.
- Disable in config: `[lint] rules = "default,-math/unbalanced-delim"`.
- Severity: non-advisory (fails `mdwright check --check`).

## References

- mdwright's math-region recogniser (`src/stdlib/math_unbalanced_delim.rs`).
- LaTeX: `\[ … \]` is the unnumbered display-math environment; `\( … \)` is the inline form.
