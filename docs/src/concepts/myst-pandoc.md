# MyST + Pandoc directives

MyST (Markedly Structured Text) is the substrate for jupyter-book and Sphinx-MyST. Pandoc has
overlapping syntax for the same shapes. mdwright recognises the common constructs from both
flavours and preserves their bytes verbatim; it does not expand directives, render roles, or
resolve substitutions. The downstream renderer (Sphinx, jupyter-book, Pandoc) does that work.

Like [Markdown extensions](extensions.md) and [math rendering](math-rendering.md), recognition is
*preservation*, not interpretation. Defaults are **on**: these recognise what the source already
says, not formatter opinion.

## What mdwright recognises

| Construct                     | Source shape                       | Default |
| ----------------------------- | ---------------------------------- | ------- |
| MyST directive container      | ``:::{name}\n…\n:::``              | on      |
| Pandoc fenced div (attr form) | ``::: {.warning}\n…\n:::``         | on      |
| Pandoc fenced div (short)     | ``:::note\n…\n:::``                | on      |
| MyST inline role              | `` {term}`Vector Space` ``         | on      |
| MyST substitution reference   | `{{name}}`                         | on      |
| Pandoc inline attribute span  | `[content]{.cls}`                  | on      |
| MyST line comment             | `% comment text`                   | on      |

Turn individual recognisers off in `.mdwright.toml` when running mdwright on non-MyST corpora:

```toml,no-check
[parse.extensions.myst]
directive-containers = false
inline-roles = false
substitution-references = false
comments = false

[parse.extensions.pandoc]
fenced-divs = false
short-form-divs = false
inline-attribute-spans = false
```

## Block directive containers

Source:

```markdown,no-check
:::{note}
This is a MyST note. It can contain *inline* and

multiple paragraphs.
:::
```

Pandoc variants (attr form and short form) are also recognised:

```markdown,no-check
::: {.warning}
Pandoc fenced div, attribute form.
:::

:::note
Pandoc short form.
:::
```

Directives with options round-trip verbatim:

```markdown,no-check
:::{figure} ./img.png
:alt: A diagram of the system
:width: 300px
:align: center

The figure caption text.
:::
```

Nested directives use opener / closer counts that increase outward: `::::` outside, `:::` inside.
mdwright preserves the nesting:

```markdown,no-check
::::{note}
Outer body.

:::{tip}
Inner body.
:::
::::
```

mdwright records the outermost directive's byte range and emits it verbatim; inner directives sit
inside that range and are preserved implicitly. Two directives at the same colon count separated
by a blank line are sibling regions, not a nested pair.

## Inline overlays

Inline roles attach a role name to a backtick-delimited payload. The role name is unrestricted:
mdwright does not know what `{term}` or `{download}` means; that is downstream's job. The bytes
round-trip:

```markdown,no-check
The {term}`Vector Space` is a fundamental concept.
```

Substitution references look the same but with double braces and no backticks:

```markdown,no-check
Some content with {{my-sub}}.
```

The declaration lives in YAML frontmatter under `myst_substitutions:` and round-trips through the
same verbatim path mdwright uses for [frontmatter](round-trip-safety.md):

```markdown,no-check
---
myst_substitutions:
  my-sub: "Replacement text"
  another: "{{my-sub}} again"
---

Body content uses {{my-sub}} and {{another}}.
```

Pandoc inline attribute spans wrap a fragment in square brackets and follow it with a brace
attribute list. mdwright distinguishes them from CommonMark links (where the brackets are followed
by `(`) and preserves the byte sequence:

```markdown,no-check
Highlight a [span of text]{.note} in the middle of a paragraph.
```

## Line comments

MyST's `%` line comment is a line whose first non-whitespace byte is `%`. mdwright preserves it
verbatim:

```markdown,no-check
% This line is dropped by MyST renderers but mdwright keeps it.
```

Unlike LaTeX, `%` is *only* a comment at the start of a line; inline `%` characters in prose are
literal text and survive untouched.

## What mdwright does not do

Expansion, role rendering, substitution resolution, and directive-name validation are all the
downstream renderer's job. A `:::{figure}` is emitted as `:::{figure}`; the image is not inlined
and the options are not rendered; `` {term}`Vector Space` `` stays as-is; `{{my-sub}}` is
preserved even when the frontmatter declares a replacement; any directive name matching
`[a-zA-Z0-9_-]+` is accepted, and an unknown name is downstream's problem.

Run mdwright before Sphinx, jupyter-book, or Pandoc: it normalises the surrounding Markdown
without touching the MyST / Pandoc constructs the downstream renderer needs.

## Round-trip and idempotence

Every MyST / Pandoc construct passes through the same idempotence-on-mode contract as the rest of
the formatter; see [Round-trip safety](round-trip-safety.md). Verbatim preservation overlays
satisfy it trivially as long as the recogniser classifies the same bytes the same way on both
passes. It does, since the scanner is fully deterministic over source bytes plus the
exclusion vectors (fenced code, inline code, HTML, math).
