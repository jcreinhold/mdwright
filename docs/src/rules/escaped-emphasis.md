---
name: escaped-emphasis
default: false
advisory: false
fix: true
since: 0.1.0
---

# escaped-emphasis

Literal `\_`, `\*`, or `` \` `` escape in prose (mdformat damage).

## What it does

Flags `\_` and `\*` escape sequences in prose that look like a writer trying to escape an
emphasis marker, but where the surrounding context confirms the writer meant the emphasis to
fire, e.g. `\_text\_` (two escapes around a word) reading as a damaged italic.

## Why

`mdformat` and a few other roundtrip tools used to defensively escape `_` and `*` in prose,
even where CommonMark would not have parsed them as emphasis. After enough roundtrips,
documents accumulate `\_word\_` patterns that no longer render as italic; they render as
literal `_word_`. The rule finds these and proposes the unescaped form.

The autofix removes the escapes (`\_text\_` → `_text_`); safe to apply, but review first if
the prose genuinely contains literal underscores (filenames, identifiers).

## Example (bad)

```markdown
This is \_actually italic\_, despite the escapes.
```

## Example (good)

```markdown
This is _actually italic_, despite the escapes.
```

## Configuration

- This rule is off by default. Enable with `[lint] rules = "default,+escaped-emphasis"`.
- Disable inline: `<!-- mdwright: allow escaped-emphasis -->`.
- Severity: non-advisory. Safe autofix available.

## References

- [CommonMark §6.2: Emphasis and strong emphasis](https://spec.commonmark.org/0.31.2/#emphasis-and-strong-emphasis).
