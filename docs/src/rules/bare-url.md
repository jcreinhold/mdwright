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

mdwright recognises GFM bare URL autolinks for rendering, but whether the same source renders as
a clickable link still depends on each downstream renderer's extension set. Wrapping the URL in
`<…>` makes the link explicit and portable across CommonMark renderers.

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
- Disable in config: `[lint] ignore = ["bare-url"]`.
- Severity: non-advisory. Safe autofix available.

## References

- [CommonMark §6.4: Autolinks](https://spec.commonmark.org/0.31.2/#autolinks).
