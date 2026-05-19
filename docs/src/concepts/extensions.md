# Markdown extensions

mdformat-mkdocs (the formatter most mkdocs-material projects reach for today) recognises a few constructs that plain
CommonMark / GFM does not. mdwright matches it for each, so a project can swap one tool for the other without visible
churn.

Recognition is *preservation*, not interpretation: mdwright knows the constructs exist, emits them canonically, and
gates each via a per-extension toggle. It does not expand abbreviations, render `{...}` to HTML, or change semantics.
The downstream renderer (Python-Markdown, mkdocs-material, jupyter-book) does that work.

GFM URL and email autolinks are recognised by default. mdwright also applies GFM tagfiltering when rendering or building
semantic signatures. These behaviours close the cmark-gfm rendering gap while keeping formatter output byte-preserving.

## The four extensions

| Extension                   | Source shape                            | Default |
| --------------------------- | --------------------------------------- | ------- |
| Definition lists            | `Term\n: definition\n`                  | on      |
| Heading attribute lists     | `# Heading {#id .class key=val}`        | on      |
| Abbreviation lists          | `*[HTML]: Hyper Text Markup Language\n` | on      |
| Non-heading attribute lists | `Paragraph\n{ .note .important }\n`     | on      |

Defaults are **on**: each recognises something the source is already doing, not a formatter opinion. Turn them off in
`.mdwright.toml` when running mdwright on non-mkdocs corpora where false positives matter more than coverage:

```toml,no-check
[parse.extensions]
definition-lists = false
abbreviation-lists = false
heading-attribute-lists = false
block-attribute-lists = false
```

## Definition lists

Source:

```markdown,no-check
Term
:   Single-paragraph definition body. Continuation lines are
    indented four spaces and aligned with the body column.

Operating system
:   The software that manages hardware resources. Notable examples:

    - Linux
    - macOS
    - Windows

    Run `uname -a` to see your kernel version.
```

Canonical emission matches mdformat-mkdocs:

- **Tight** form (`Term\n:   body`) for single-paragraph definitions.
- **Loose** form (blank line between term and the `:` marker) when the definition has multiple block children: a
  paragraph plus a nested list / code block, or multi-paragraph text. The blank line is the syntactic boundary that
  makes the multi-block body parse correctly.

Multiple definitions for one term emit on consecutive `:   ` lines with no blank between them; blank lines separate term
groups.

## Heading attribute lists

Source:

```markdown,no-check
# Heading {#section-one}

## Multiple classes {.warning .important}

### Mixed shape {#mix .alpha .beta key=val}
```

The trailer parses through pulldown-cmark's `ENABLE_HEADING_ATTRIBUTES` flag, lands on the typed `Heading`, and re-emits
based on `[fmt] heading-attrs`:

| Mode                 | Behaviour |
| -------------------- | --------- |
| `preserve` (default) | Emit the source trailer byte-verbatim between the inline body and the line break. |
| `canonicalise`       | Emit `{#id .class₁ .class₂ k=v}`: id first, then classes (source order), then `key=value` pairs (source order). Values containing whitespace are double-quoted. |

```toml,no-check
[fmt]
heading-attrs = "preserve"  # or "canonicalise"
```

**Pulldown limitation.** pulldown-cmark 0.13's heading-attribute parser splits the trailer on whitespace and does not
honour double-quoted values. `# H {title="hello world"}` parses as two attributes, `title="hello` and `world"`, not one.
mdformat-mkdocs (which uses python-markdown's `attr_list`) handles the quoted form correctly. Until pulldown upstream
lands the fix, mdwright's heading-attribute output for quoted values diverges from mdformat-mkdocs; documented in
[Deviations from spec](../deviations.md).

## Abbreviation lists

Source:

```markdown,no-check
The HTML standard is maintained by the W3C.

*[HTML]: Hyper Text Markup Language
*[W3C]: World Wide Web Consortium
```

mdwright recognises the `*[TERM]: definition` shape and preserves the declarations verbatim. It does **not** expand
occurrences (the downstream renderer wraps them in `<abbr title="…">…</abbr>`). Each declaration is one source line;
continuation lines are not supported, matching python-markdown's `abbr` extension.

Consecutive abbreviation lines (no blank line between them) are bundled into one source paragraph by pulldown and
emitted as one verbatim block. A blank line above the first declaration is conventional but not required.

## Non-heading attribute lists

Source:

```markdown,no-check
This paragraph carries a class trailer used by the renderer to style it.
{ .note .important }
```

The trailer must:

- sit on the line immediately after a non-empty block (no blank-line separator), and
- contain only the brace-delimited attribute list and optional surrounding whitespace.

When mdwright recognises the pattern, the entire block (body + trailer) is emitted as a single verbatim source slice.
Other paragraph-level rewrites (line wrap, link normalisation, escape rewrites) are skipped for that paragraph, so
preservation narrows the formatter's active surface for the formatter on annotated blocks.

**Inline attribute lists** (`some *emphasised* { .em } text` mid-paragraph) are explicitly out of scope. mdwright's
inline formatter has no overlay mechanism today; adding one is a separate design exercise. Inline `{...}` tokens flow
through as plain text.

## Round-trip and idempotence

Reformatting under any combination of these extensions still goes through the
[HTML-equivalence gate](round-trip-safety.md). Verbatim overlays satisfy it trivially, and the canonical emission shape
for typed-block constructs is a fixed point of its own parser by construction.

## Parity with mdformat-mkdocs

The parity goal is concrete: an mkdocs-material site running mdformat-mkdocs swaps in mdwright with no visible
diff. The parity test at `tests/extension_parity.rs` byte-compares mdwright's output against mdformat-mkdocs
reference output for the five extension regression fixtures; any divergence is fixed in mdwright or recorded in
[Deviations from spec](../deviations.md).
