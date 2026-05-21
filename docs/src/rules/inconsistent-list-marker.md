---
name: inconsistent-list-marker
default: true
advisory: false
fix: false
since: 0.1.0
---

# inconsistent-list-marker

Mixed `-` / `*` / `+` markers in one bullet list.

## What it does

Flags bulleted lists whose items use a mix of marker characters (`-`, `*`, `+`) within the same
list. Ordered lists are not affected.

## Why

CommonMark says any of `-`, `*`, `+` is a valid bullet marker; switching markers between items
either (a) reads as a typo, or (b) actually starts a _new_ list under CommonMark's rules, which
produces a visible gap in the rendered output that the author rarely intended. Either way the
fix is to pick one marker per list and stick to it.

The formatter normalises markers across the whole document; this rule fires when the source as
written would render two adjacent lists when one was meant.

## Example (bad)

```markdown
- first
* second
- third
```

## Example (good)

```markdown
- first
- second
- third
```

## Configuration

- Disable inline: `<!-- mdwright: allow inconsistent-list-marker -->`.
- Disable in config: `[lint] ignore = ["inconsistent-list-marker"]`.
- Severity: non-advisory.

## References

- [CommonMark §5.2: List items](https://spec.commonmark.org/0.31.2/#list-items).
- `[fmt] list-marker` config key controls the canonical marker for the formatter.
