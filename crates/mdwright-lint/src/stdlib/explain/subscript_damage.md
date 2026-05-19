## What it does

Flags damaged subscript notation produced by older roundtrip Markdown tools—patterns like
`x\_i` (escaped underscore that was supposed to be a TeX subscript) where the surrounding
context confirms a math intent (digits, single-letter identifiers, sign-and-digit pairs).

## Why

Older `mdformat` versions and a few other tools defensively escaped `_` inside what looked
like prose, even when the underscore was a math subscript. The result is `x\_i` in the
source, which renders as `x_i` literally—not as a subscript. The rule finds these and
proposes either the TeX form (`x_i` inside math) or the Unicode form (`xᵢ`) depending on
context.

The autofix is conservative: it removes the backslash if the surrounding context is
unambiguously math; otherwise it leaves the source untouched and prints the diagnostic
only.

## Example (bad)

```markdown
Take the i-th element x\_i.
```

## Example (good)

```markdown
Take the i-th element xᵢ.
```

(Or, inside math: `Take the i-th element $x_i$.`)

## Configuration

- This rule is off by default. Enable with `[lint] rules = "default,+subscript-damage"`.
- Disable inline: `<!-- mdwright: allow subscript-damage -->`.
- Severity: non-advisory. Safe autofix where context is unambiguous.

## References

- See also [`unicodeable-subscript`](unicodeable-subscript.md) for substituting Unicode
  subscript characters in prose.
