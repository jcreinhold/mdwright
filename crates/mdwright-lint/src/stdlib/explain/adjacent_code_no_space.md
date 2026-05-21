## What it does

Flags inline code spans whose backticks touch an adjacent alphanumeric or backtick character
without an intervening space, e.g. `` `foo`bar `` or `` `foo``bar` ``.

## Why

CommonMark renders `` `foo`bar `` as `<code>foo</code>bar`; the visual result runs the code
into the prose with no visual break, which is almost always a typo for `` `foo` bar `` or
`` `foobar` ``. Two consecutive code spans with no space between them
(`` `foo``bar` ``) is even more ambiguous: it depends on backtick counting and renders
inconsistently across implementations.

## Example (bad)

```markdown
Call `vec.push(x)`afterwards.
```

## Example (good)

```markdown
Call `vec.push(x)` afterwards.
```

## Configuration

- Disable inline: `<!-- mdwright: allow adjacent-code-no-space -->`.
- Disable in config: `[lint] ignore = ["adjacent-code-no-space"]`.
- Severity: non-advisory.

## References

- [CommonMark §6.1: Code spans](https://spec.commonmark.org/0.31.2/#code-spans).
