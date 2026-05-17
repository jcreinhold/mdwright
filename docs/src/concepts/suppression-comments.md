# Suppression comments

A suppression comment silences one lint rule on the next block, the next line, or a range. They
look like HTML comments so they are invisible in the rendered document.

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

**Range.** Open with `allow-begin`, close with `allow-end`. Useful for tables, generated content,
or vendored sections:

```markdown
<!-- mdwright: allow-begin bare-url -->

| Source | URL |
| --- | --- |
| Spec | https://spec.commonmark.org/ |
| GFM | https://github.github.com/gfm/ |

<!-- mdwright: allow-end bare-url -->
```

**Multiple rules.** Separate with commas: `<!-- mdwright: allow bare-url, latex-command -->`.

**All rules.** Use the literal `all` (rarely the right choice): `<!-- mdwright: allow all -->`.

## Auditing what you have silenced

```sh
mdwright check --no-suppress .
```

ignores every suppression marker and reports the full diagnostic set. Use this to spot-check that
you are not suppressing something that became a real bug after a refactor.

`mdwright check` itself reports unused suppressions: a `<!-- mdwright: allow bare-url -->` that
no longer applies (because the next block has no bare URLs) surfaces as an advisory so you can
delete the marker.

## Choosing suppression over disabling

A suppression marker is the right choice when you want a rule enabled project-wide and silenced at
one location with a stated reason. Add a sibling HTML comment explaining why:

```markdown
<!-- mdwright: allow bare-url -->
<!-- The renderer in this project linkifies bare URLs itself. -->

See https://example.com for the spec.
```

When you find yourself suppressing the same rule in dozens of places, disable it in
configuration:

```toml,no-check
[lint]
rules = "default,-bare-url"
```

See [Configuration](../configuration.md#lint).

## See also

- [Lint vs. format](lint-vs-format.md) — suppression only affects linting; the formatter has no
  per-document opt-out.
- [Rules catalogue](../rules/index.md) — every rule's kebab-case name (the literal that goes in
  the suppression comment).
