---
name: heading-punctuation
default: true
advisory: false
fix: false
since: 0.1.0
---

# heading-punctuation

Trailing `.` or `:` on a heading.

## What it does

Flags ATX or setext headings that end with `.`, `!`, or `?`—terminal sentence punctuation.

## Why

A heading is a title, not a sentence; terminal punctuation on titles reads as a typo, breaks
heading-anchor slugs in some renderers, and inflates the table of contents with stray
characters. The convention is shared by Microsoft, GitHub, and Google's documentation style
guides.

## Example (bad)

```markdown
## Configuring the linter.
```

## Example (good)

```markdown
## Configuring the linter
```

If the heading genuinely needs to be a question, this rule still fires; either reword it as a
declarative title or suppress on that block.

## Configuration

- Disable inline (for one heading): place `<!-- mdwright: allow heading-punctuation -->`
  on the line before the heading.
- Disable in config: `[lint] rules = "default,-heading-punctuation"`.
- Severity: non-advisory.

## References

- [Microsoft Writing Style Guide—Capitalization, headings, and titles](https://learn.microsoft.com/en-us/style-guide/capitalization).
