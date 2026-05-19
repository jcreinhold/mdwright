## What it does

Flags lists whose items mix tight (single-paragraph) and loose (blank-line-separated) shapes
across the same list, leaving CommonMark's spec-defined "tightness" algorithm to make a
surprising choice.

## Why

CommonMark decides a list is "loose" if _any_ item has a blank line between it and the next.
That single blank line then re-renders _every_ item with `<p>` wrappers, which adds vertical
padding throughout the list. Authors who write one stray blank line frequently don't notice the
cascading effect on items above and below.

This rule is advisory because the "wrong" tightness is rarely a bug per se, but the surprise
is consistent enough that flagging it is worth a one-line nudge.

## Example (bad)

```markdown
- first
- second

- third
```

(Loose: every item gets `<p>` wrappers because of the blank line above `third`.)

## Example (good)

Tight throughout:

```markdown
- first
- second
- third
```

Or loose throughout:

```markdown
- first

- second

- third
```

## Configuration

- This rule is off by default (opt-in). Enable with
  `[lint] rules = "default,+list-tightness-flipped"`.
- Disable inline: `<!-- mdwright: allow list-tightness-flipped -->`.
- Severity: advisory (does not fail `--check`).

## References

- [CommonMark §5.3: Lists](https://spec.commonmark.org/0.31.2/#lists).
