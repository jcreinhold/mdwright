---
name: duplicate-link-label
default: true
advisory: false
fix: false
since: 0.1.0
---

# duplicate-link-label

Two `[label]:` definitions with the same label.

## What it does

Flags `[label]: …` link definitions that share a label (case-insensitive, normalised) with
another definition in the same document.

## Why

CommonMark says the first definition wins; later duplicates are silently discarded. The author
usually intended for one of them to be a different label, so a duplicate is almost always a
copy-paste mistake. Worse, the discarded definition often documents the intended target, so
the link still resolves, but to the wrong URL.

## Example (bad)

```markdown
See the [docs][readme] and the [tutorial][readme].

[readme]: https://example.com/readme
[readme]: https://example.com/tutorial
```

## Example (good)

```markdown
See the [docs][readme] and the [tutorial][tutorial].

[readme]: https://example.com/readme
[tutorial]: https://example.com/tutorial
```

## Configuration

- Disable inline: `<!-- mdwright: allow duplicate-link-label -->`.
- Disable in config: `[lint] ignore = ["duplicate-link-label"]`.
- Severity: non-advisory.

## References

- [CommonMark §4.7: Link reference definitions](https://spec.commonmark.org/0.31.2/#link-reference-definitions).
