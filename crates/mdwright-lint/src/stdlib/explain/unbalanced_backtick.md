## What it does

Flags inline code spans whose backtick fence was not closed before the end of a paragraph or the
end of the document, e.g. `` `foo `` with no matching `` ` ``.

## Why

CommonMark's inline code rule requires the same number of opening and closing backticks. An
unclosed opener silently turns the rest of the paragraph into prose (it is _not_ rendered as a
code span), so the visual result drifts from what the author meant. Worse, the unclosed run
often eats markup intended for later constructs (links, emphasis), producing a cascade of
silently broken rendering.

## Example (bad)

```markdown
Run `cargo build to compile.
```

## Example (good)

```markdown
Run `cargo build` to compile.
```

## Configuration

- Disable inline: `<!-- mdwright: allow unbalanced-backtick -->`.
- Disable in config: `[lint] rules = "default,-unbalanced-backtick"`.
- Severity: non-advisory.

## References

- [CommonMark §6.1: Code spans](https://spec.commonmark.org/0.31.2/#code-spans).
