---
name: unicodeable-subscript
default: true
advisory: true
fix: true
since: 0.2.0
---

# unicodeable-subscript

Braced super/subscript that has a single-codepoint Unicode form.

## What it does

Flags math-region subscripts whose contents are simple enough to express as Unicode subscript
characters: single digits, single letters that have a Unicode subscript codepoint, and short
sign/digit pairs, when the surrounding context is prose rather than display math.

## Why

In running prose like `the i-th element x_i`, the TeX form `x_i` renders as a code-styled
fragment in most renderers; the Unicode form `xᵢ` is cleaner, screen-reader-friendly, and
copyable. The rule fires only when the substitution is unambiguous and the surrounding context
does not already use TeX-style display math.

The autofix substitutes the Unicode form (`safe: false`); review the change before applying.

## Example (bad)

```markdown
Take the i-th component, x_i.
```

## Example (good)

```markdown
Take the i-th component, xᵢ.
```

## Configuration

- Disable inline: `<!-- mdwright: allow unicodeable-subscript -->`.
- Disable in config: `[lint] rules = "default,-unicodeable-subscript"`.
- Severity: advisory.

## References

- Unicode subscript block: `U+2080`–`U+209F`.
- mdwright's substitution table (`mdwright-math/src/unicode.rs`).
