# MyST + Pandoc directives

MyST (Markedly Structured Text) is the substrate for jupyter-book and Sphinx-MyST. Pandoc has overlapping syntax for
the same shapes. mdwright recognises the common constructs from both flavours and preserves their bytes verbatim —
mdwright does not expand directives, render roles, or interpret substitutions. The downstream renderer (Sphinx,
jupyter-book, Pandoc itself) does that work.

Like [Markdown extensions](extensions.md) and [math rendering](math-rendering.md), recognition is *preservation*,
not interpretation. Defaults are **on**: these recognise what the source already says, not formatter opinion.

## What mdwright recognises

| Construct                     | Source shape                                      | Default | Mechanism                |
| ----------------------------- | ------------------------------------------------- | ------- | ------------------------ |
| MyST directive container      | ``:::{name}\n…\n:::``                             | on      | scan-and-preserve (block) |
| Pandoc fenced div (attr form) | ``::: {.warning}\n…\n:::``                        | on      | scan-and-preserve (block) |
| Pandoc fenced div (short)     | ``:::note\n…\n:::``                               | on      | scan-and-preserve (block) |
| MyST inline role              | `` {term}`Vector Space` ``                        | on      | scan-and-preserve (inline) |
| MyST substitution reference   | `{{name}}`                                        | on      | scan-and-preserve (inline) |
| Pandoc inline attribute span  | `[content]{.cls}`                                 | on      | scan-and-preserve (inline) |
| MyST line comment             | `% comment text`                                  | on      | scan-and-preserve (block) |

Turn individual recognisers off in `.mdwright.toml` when running mdwright on non-MyST corpora and avoiding
false-positives matters more than coverage:

```toml,no-check
[fmt.extensions.myst]
directive-containers = false
inline-roles = false
substitution-references = false
comments = false

[fmt.extensions.pandoc]
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

Nested directives use opener / closer counts that increase outward — `::::` outside, `:::` inside — and mdwright
preserves the nesting:

```markdown,no-check
::::{note}
Outer body.

:::{tip}
Inner body.
:::
::::
```

mdwright records the outermost directive's byte range and emits it verbatim; inner directives sit inside that range
and are preserved implicitly. Two directives at the same colon count separated by a blank line are sibling regions,
not a nested pair.

## Inline overlays

Inline roles attach a role name to a backtick-delimited payload. The role name is unrestricted: mdwright does not
know what `{term}` or `{download}` means; that's downstream's job. The bytes round-trip:

```markdown,no-check
The {term}`Vector Space` is a fundamental concept.
```

Substitution references look the same as block directives but with double braces and no backticks:

```markdown,no-check
Some content with {{my-sub}}.
```

The declaration lives in YAML frontmatter under `myst_substitutions:` and round-trips through the same verbatim
path mdwright already uses for [frontmatter](round-trip-safety.md):

```markdown,no-check
---
myst_substitutions:
  my-sub: "Replacement text"
  another: "{{my-sub}} again"
---

Body content uses {{my-sub}} and {{another}}.
```

Pandoc inline attribute spans wrap a fragment in square brackets and follow it with a brace attribute list. mdwright
distinguishes them from CommonMark links (where the brackets are followed by `(`) and preserves the byte sequence:

```markdown,no-check
Highlight a [span of text]{.note} in the middle of a paragraph.
```

## Line comments

MyST's `%` line comment is a line whose first non-whitespace byte is `%`. mdwright preserves the line verbatim:

```markdown,no-check
% This line is dropped by MyST renderers but mdwright keeps it.
```

Unlike LaTeX, `%` is *only* a comment when it sits at the start of a line; inline `%` characters in prose are
literal text and survive untouched.

## What mdwright does not do

- **Expand directives.** A `:::{figure}` is emitted as `:::{figure}`; the figure image is not inlined and the
    options are not rendered.
- **Render inline roles.** `` {term}`Vector Space` `` stays as `` {term}`Vector Space` ``; mdwright does not look
    up the role definition or substitute a rendered span.
- **Resolve substitutions.** `{{my-sub}}` is preserved as-is even when the frontmatter declares a replacement.
- **Validate directive arguments.** mdwright accepts any directive name `[a-zA-Z0-9_-]+`; an unknown name is
    downstream's problem.

If you need any of those operations, run mdwright before Sphinx, jupyter-book, or Pandoc: it normalises the
surrounding Markdown without touching the MyST / Pandoc constructs the downstream renderer needs.

## Round-trip and idempotence

Every MyST / Pandoc construct goes through the same idempotence-on-mode contract as the rest of the formatter:
`format(format(src, opts), opts) == format(src, opts)`. Verbatim preservation overlays hold this contract trivially
as long as the recogniser classifies the same bytes the same way on both passes. The scanner is fully deterministic
— it consumes source bytes plus the exclusion vectors (fenced code, inline code, HTML, math) and produces the same
regions every time — so reformatting a MyST document twice is guaranteed to be a fixed point.
