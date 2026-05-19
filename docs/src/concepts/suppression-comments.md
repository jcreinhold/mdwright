# Suppression comments

A suppression comment silences one lint rule on the next block, the next line, or a range. They look like HTML comments
so they are invisible in the rendered document.

## Forms

**Next block.** Silence one rule on the block immediately following:

```markdown
<!-- mdwright: allow bare-url -->

See https://example.com for the spec.
```

**Next line.** Silence on the next non-blank line only:

```markdown
<!-- mdwright: allow-next-line bare-url -->
See https://example.com for the spec.
```

**Range.** Open with `allow-begin`, close with `allow-end`. Useful for tables, generated content, or vendored sections:

```markdown
<!-- mdwright: allow-begin bare-url -->

| Source | URL |
| --- | --- |
| Spec | https://spec.commonmark.org/ |
| GFM | https://github.github.com/gfm/ |

<!-- mdwright: allow-end bare-url -->
```

Separate multiple rules with commas: `<!-- mdwright: allow bare-url, latex-command -->`. Use the literal `all` to
silence every rule (rarely the right choice): `<!-- mdwright: allow all -->`.

## Auditing what you have silenced

```sh
mdwright check --no-suppress .
```

ignores every suppression marker and reports the full diagnostic set. Use this to find suppressions that no longer
correspond to a real diagnostic.

`mdwright check` itself reports unused suppressions: a `<!-- mdwright: allow bare-url -->` whose target block has no
bare URLs surfaces as an advisory, so you can delete the marker.

## Suppression vs. disabling

Use a suppression marker when a rule is right project-wide but wrong at one location, and add a sibling HTML comment
explaining why:

```markdown
<!-- mdwright: allow bare-url -->
<!-- The renderer in this project linkifies bare URLs itself. -->

See https://example.com for the spec.
```

When the same suppression appears in dozens of places, disable the rule in configuration instead:

```toml,no-check
[lint]
rules = "default,-bare-url"
```

See [Configuration](../configuration.md#lint).

## See also

- [Lint vs. format](lint-vs-format.md): suppression only affects linting; the formatter has no per-document opt-out.
- [Rules catalogue](../rules/index.md): every rule's kebab-case name (the literal that goes in the suppression comment).
