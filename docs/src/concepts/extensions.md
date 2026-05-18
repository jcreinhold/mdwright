# Markdown extensions

mdformat-mkdocs (the formatter most mkdocs-material projects reach for today) recognises a handful of constructs that
plain CommonMark / GFM does not: definition lists, abbreviation declarations, and attribute-list trailers. mdwright
matches mdformat-mkdocs for each of these so a project can swap one tool for the other without visible churn in the
formatted output.

Like [math rendering](math-rendering.md), recognition is *preservation*, not interpretation. mdwright knows the
constructs exist, formats them canonically, and gates them via per-extension toggles in `.mdwright.toml`; it does not
expand abbreviations, render `{...}` into HTML, or change semantics. The downstream renderer (Python-Markdown,
mkdocs-material, jupyter-book) does that work.

## The four extensions

| Extension                | Source shape                            | Default | Mechanism                                |
| ------------------------ | --------------------------------------- | ------- | ---------------------------------------- |
| Definition lists         | `Term\n: definition\n`                  | on      | pulldown `ENABLE_DEFINITION_LIST` events |
| Heading attribute lists  | `# Heading {#id .class key=val}`        | on      | pulldown `ENABLE_HEADING_ATTRIBUTES` fields |
| Abbreviation lists       | `*[HTML]: Hyper Text Markup Language\n` | on      | scan-and-preserve overlay                |
| Non-heading attribute lists | `Paragraph\n{ .note .important }\n`  | on      | scan-and-preserve overlay                |

Defaults are **on**: these are features the source already uses, not formatter opinions. Turn them off in
`.mdwright.toml` when you run mdwright on non-mkdocs corpora and don't want any of the four to fire by accident:

```toml,no-check
[fmt.extensions]
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
- **Loose** form (blank line between term and the `:` marker) when the definition has multiple block children — a
    paragraph plus a nested list / code block, or multi-paragraph text. The blank line is the syntactic boundary that
    makes the multi-block body parse correctly.

Multiple definitions for one term emit on consecutive `:   ` lines with no blank line between them; blank lines
separate term groups.

## Heading attribute lists

Source:

```markdown,no-check
# Heading {#section-one}

## Multiple classes {.warning .important}

### Mixed shape {#mix .alpha .beta key=val}
```

The trailer parses through pulldown-cmark's `ENABLE_HEADING_ATTRIBUTES` flag, lands on the typed `Heading`, and re-emits
based on `[fmt] heading-attrs`:

| Mode | Behaviour |
| --- | --- |
| `preserve` (default) | Emit the source trailer byte-verbatim between the rendered inline body and the line break. |
| `canonicalise`       | Emit `{#id .class₁ .class₂ k=v}` — id first, then classes (source order), then `key=value` pairs (source order). Values containing whitespace are double-quoted. |

In `.mdwright.toml`:

```toml,no-check
[fmt]
heading-attrs = "preserve"  # or "canonicalise"
```

**Pulldown limitation**: pulldown-cmark 0.13's heading-attribute parser splits the trailer on whitespace and does not
honour double-quoted values. `# H {title="hello world"}` parses as two attributes — `title="hello` and `world"` — not
one. mdformat-mkdocs (which uses python-markdown's `attr_list`) handles the quoted form correctly. Until pulldown
upstream lands the fix, mdwright's heading-attribute output for quoted values diverges from mdformat-mkdocs; documented
in [Deviations from spec](../deviations.md).

## Abbreviation lists

Source:

```markdown,no-check
The HTML standard is maintained by the W3C.

*[HTML]: Hyper Text Markup Language
*[W3C]: World Wide Web Consortium
```

mdwright recognises the `*[TERM]: definition` shape and preserves the declarations verbatim. It does **not** expand
occurrences (the downstream renderer wraps them in `<abbr title="…">…</abbr>`). Each declaration is one source line —
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

- Be on the line immediately after a non-empty block (no blank-line separator), AND
- Contain only the brace-delimited attribute list and optional surrounding whitespace.

When mdwright recognises the pattern, the entire block (body + trailer) is emitted as a single verbatim source slice.
Other paragraph-level rewrites (line wrap, link normalisation, escape rewrites) are skipped for that paragraph — the
price of preservation is a narrower active surface for the formatter on annotated blocks.

**Inline attribute lists** (`some *emphasised* { .em } text` mid-paragraph) are explicitly out of scope. mdwright's
inline formatter has no overlay mechanism today; adding one is a separate design exercise. Inline `{...}` tokens flow
through as plain text.

## The HTML-equivalence gate

Every reformat under `mdwright fmt` runs through the same HTML-equivalence gate that math rendering uses — the
*idempotence-on-mode* contract. Formatting the output a second time with the same options must produce the same
canonical event stream. Round-1-to-round-2 divergence is a hard failure regardless of which extension was active.

For scan-and-preserve overlays (abbreviations, non-heading attribute lists) idempotence holds for free: source bytes
flow through unchanged on the first pass, parse identically on the second, and produce the same overlay decisions. For
typed-block constructs (definition lists, heading attribute trailers) the canonical emission shape is a fixed point of
its own parser by construction.

## Parity with mdformat-mkdocs

The parity goal is concrete: an mkdocs-material site that today runs mdformat-mkdocs swaps in mdwright with no visible
diff in the formatted output. The parity test at `tests/extension_parity.rs` byte-compares mdwright's output against
mdformat-mkdocs reference output for the five extension regression fixtures, and any divergence is either fixed in
mdwright or recorded as a row in [Deviations from spec](../deviations.md).
