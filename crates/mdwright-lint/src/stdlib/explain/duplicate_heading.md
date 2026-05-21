## What it does

Flags two or more headings whose slug (lowercase, hyphenated text) collide within the same
document.

## Why

Markdown renderers (GitHub, mdBook, GitLab) assign each heading a URL fragment derived from its
text. Two headings with the same text collide on the fragment: only one is reachable, and which
one depends on whether the renderer disambiguates with a `-1` suffix or silently overwrites.
External links to the document then drift unpredictably as new sections are added.

## Example (bad)

```markdown
## Examples

…

## Examples
```

## Example (good)

```markdown
## Examples

…

## More examples
```

## Configuration

- Disable inline: `<!-- mdwright: allow duplicate-heading -->`.
- Disable in config: `[lint] ignore = ["duplicate-heading"]`.
- Severity: advisory. Math/theorem documents legitimately repeat `### Proof` or `### Corollary`
  under one chapter, so the diagnostic surfaces but does not fail `mdwright check --check`.

## References

- GitHub's slug algorithm: lowercase, replace whitespace with `-`, strip non-word characters.
