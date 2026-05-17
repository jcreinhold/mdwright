---
name: stray-dollar
default: false
advisory: false
fix: true
since: 0.1.0
---

# stray-dollar

Literal `$` in prose (opt-in for projects that don't use $…$ math).

## What it does

Flags `$` characters in prose that are not part of a recognised math region.

## Why

Some Markdown renderers (Pandoc with `--mathjax`, KaTeX-aware mdBook, GitHub) treat `$…$` as
inline math. Authors who rely on TeX-style `\( … \)` instead can accidentally produce math
output where they wanted a literal `$5`, and authors who rely on `$…$` for math can produce
prose where they meant math. Either way, a stray single `$` in prose is almost always a typo.

This rule is opt-in because projects that consistently use `$…$` for math have no use for it —
the linter would flood with false positives. Turn it on in projects that standardise on
`\( … \)` or no inline math at all.

The autofix escapes the dollar (`\$`); review before applying.

## Example (bad)

```markdown
That costs $5.
```

## Example (good)

```markdown
That costs \$5.
```

## Configuration

- This rule is off by default. Enable with `[lint] rules = "default,+stray-dollar"`.
- Disable inline: `<!-- mdwright: allow stray-dollar -->`.
- Severity: non-advisory. Safe autofix available.

## References

- Pandoc Markdown — math extension.
- mdwright treats `\( … \)`, `\[ … \]`, and named environments as math regions; `$…$` is
  excluded by default because it conflicts with literal-dollar prose.
