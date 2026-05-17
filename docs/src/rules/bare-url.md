---
name: bare-url
default: true
advisory: false
fix: true
since: 0.1.0
---

# bare-url

Bare URL in prose; wrap in `<…>` for a CommonMark autolink.

## What it does

Flags `http://` and `https://` URLs that appear in prose without being wrapped in a CommonMark
autolink (`<https://example.com>`) or a `[text](url)` link.

## Why

Whether a bare URL renders as a clickable link depends entirely on the renderer's autolink
heuristics, which differ across GitHub, GitLab, Pandoc, KaTeX-aware mdBook, and editor
previews. Wrapping the URL in `<…>` makes the link explicit and portable.

The autofix (`safe: true`) wraps the URL in angle brackets in place; `mdwright fix` applies it.

## Example (bad)

```markdown
See https://example.com for details.
```

## Example (good)

```markdown
See <https://example.com> for details.
```

## Configuration

- Disable inline: `<!-- mdwright: allow bare-url -->`.
- Disable in config: `[lint] rules = "default,-bare-url"`.
- Severity: non-advisory. Safe autofix available.

## References

- [CommonMark §6.4 — Autolinks](https://spec.commonmark.org/0.31.2/#autolinks).
