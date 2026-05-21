## What it does

Flags reference-style links of the form `[text][label]` or shortcut form `[label]` where
`label` has no matching `[label]: …` definition anywhere in the document.

## Why

CommonMark renders an unresolved reference link as literal text; `[text][label]` shows up in
the output as `[text][label]` rather than as a clickable link. This silently breaks navigation,
usually because the author renamed a link definition without updating its references (or vice
versa).

## Example (bad)

```markdown
See the [installation guide][install] for details.

[setup]: docs/setup.md
```

## Example (good)

```markdown
See the [installation guide][install] for details.

[install]: docs/install.md
```

## Configuration

- Disable inline: `<!-- mdwright: allow orphan-reference-link -->`.
- Disable in config: `[lint] ignore = ["orphan-reference-link"]`.
- Severity: non-advisory.

## References

- [CommonMark §6.5: Links](https://spec.commonmark.org/0.31.2/#links).
