## What it does

Flags lines that end with one or more trailing space or tab characters. The exception is the
CommonMark hard-break convention: exactly two trailing spaces followed by a newline introduces
a `<br>` inside a paragraph, and that case is left alone.

## Why

Trailing whitespace is invisible noise that survives copy-paste, complicates diffs (one-byte
changes that touch every line), and frequently triggers spurious changes when collaborators
have different editor settings. The autofix strips the trailing run while preserving the
two-space hard break form.

## Example (bad)

```text
A paragraph.···
Another line.·
```

(`·` represents a stray trailing space.)

## Example (good)

```text
A paragraph.
Another line.
```

## Configuration

- Disable inline: `<!-- mdwright: allow trailing-whitespace -->`.
- Disable in config: `[lint] rules = "default,-trailing-whitespace"`.
- Severity: non-advisory. Safe autofix available.

## References

- [CommonMark §6.7: Hard line breaks](https://spec.commonmark.org/0.31.2/#hard-line-breaks).
