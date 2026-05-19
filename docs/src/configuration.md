# Configuration

mdwright reads configuration from (in precedence order):

1. The file given via `--config PATH`.
2. The nearest ancestor config discovered by walking upward from the
   current directory. At each ancestor, candidates are tried in this
   order: `.mdwright.toml`, `mdwright.toml`,
   `pyproject.toml` containing a `[tool.mdwright]` table. The walk
   stops at the filesystem root or at the first directory containing
   `.git/` (the workspace boundary).
3. Built-in defaults.

A `pyproject.toml` *without* `[tool.mdwright]` does not stop the walk;
discovery continues to the parent directory. A `.mdwright.toml` wins
over a `pyproject.toml` in the same directory (matching ruff's
"more-specific-name first" rule).

## Single-file integration via `pyproject.toml`

For projects that already use `pyproject.toml`, the entire mdwright
configuration can live there under `[tool.mdwright]`:

```toml
# pyproject.toml
[tool.mdwright]
lint.rules = "default,+latex-command"

[tool.mdwright.fmt]
wrap = 100
```

## CLI overrides

The following knobs accept CLI flags that take precedence over the
config file:

- `lint.rules`: `--rules`
- `--no-suppress` toggles whether `<!-- mdwright: allow ... -->`
  comments are honoured; there is no config-file equivalent.

All other `[fmt]` knobs are config-file-only.

## Schema reference

<!-- BEGIN GENERATED: do not edit. Regenerate by running `cargo xtask doc-config`. -->

### `[lint]` and nested tables

| Key | Type | Default | CLI override | Description |
| --- | --- | --- | --- | --- |
| `lint.rules` | string | `"default"` | `--rules` | Rule-selection spec. Comma-separated tokens: `all`, `default`, `<name>` (start from `{<name>}`), `+<name>` (add), `-<name>` (remove). |
| `lint.exclude` | array of string | `[]` | `none` | Gitignore-style patterns. Matching files are dropped from lint runs. Patterns are anchored to the directory containing the config file. |
| `lint.info-strings.extra` | array of string | `[]` | `none` | Project-specific additions to the `info-string-typo` allowlist. The stdlib's default allowlist still applies. |

### `[fmt]` and nested tables

| Key | Type | Default | CLI override | Description |
| --- | --- | --- | --- | --- |
| `fmt.wrap` | "keep" \| "no" \| int | `"keep"` | `none` | Wrap mode for prose paragraphs. `keep` leaves existing breaks alone; `no` forbids new breaks; an integer wraps at that column. |
| `fmt.italic` | "asterisk" \| "underscore" \| "preserve" | `"preserve"` | `none` | Italic delimiter canonicalisation. `preserve` (default) leaves source bytes; `asterisk` / `underscore` opt into the post-pass rewrite. See [Style knobs](format/style.md). |
| `fmt.strong` | "asterisk" \| "underscore" \| "preserve" | `"preserve"` | `none` | Strong-emphasis delimiter canonicalisation. Independent of `fmt.italic`: `*italic*` with `__strong__` is expressible. |
| `fmt.list-marker` | "dash" \| "asterisk" \| "plus" \| "preserve" | `"preserve"` | `none` | Unordered-list bullet canonicalisation. Every bullet in one list rewrites together or none do. |
| `fmt.ordered-list` | "consistent" \| "preserve" | `"preserve"` | `none` | Ordered-list number canonicalisation. `consistent` renumbers each list to a clean ascending run starting from the source's first item's number; `preserve` keeps source numbering verbatim. |
| `fmt.thematic-break` | "dash" \| "asterisk" \| "underscore" \| "preserve" | `"preserve"` | `none` | Thematic-break canonicalisation. Rewrites the repeated character (`---` ↔ `***` ↔ `___`); the repeat count and internal spacing stay source. |
| `fmt.trailing-newline` | "preserve" \| "strip" \| "ensure" \| bool | `"preserve"` | `none` | Trailing-newline policy at the document boundary. `true` is accepted as a synonym for `ensure` and `false` for `strip` (legacy schema). |
| `fmt.end-of-line` | "lf" \| "crlf" \| "keep" | `"lf"` | `none` | Line-ending normalisation. `keep` adopts the first newline seen in the source. |
| `fmt.exclude` | array of string | `[]` | `none` | Formatter-specific exclude globs, independent of `[lint] exclude`. |
| `fmt.refs.placement` | "end" \| "preserve" | `"end"` | `none` | Where reference-link definitions are emitted: gathered and sorted at the end of the document, or kept in source order. |
| `fmt.refs.style` | "bare" \| "angle" \| "preserve" | `"preserve"` | `none` | Destination style for reference-link and inline-link URLs. `preserve` (default) keeps each destination's source form; `bare` strips wrapping `<…>` where the bare form would still parse; `angle` wraps every destination in `<…>`. |
| `fmt.footnotes.placement` | "end" \| "preserve" | `"preserve"` | `none` | Where footnote definitions are emitted. Default is `preserve` because pulldown-cmark's HTML renderer ties footnote position to parse order; moving definitions would change the rendered HTML. |
| `fmt.frontmatter.preserve` | bool | `true` | `none` | Whether to emit document frontmatter byte-verbatim. `false` strips it. |
| `fmt.heading-attrs` | "preserve" \| "canonicalise" | `"preserve"` | `none` | ATX heading `{#id .class key=val}` trailer emission. `preserve` (default) emits the source trailer byte-verbatim. `canonicalise` emits id first, then classes (source order), then key=value pairs (source order). See [Markdown extensions](concepts/extensions.md#heading-attribute-lists). |

### `[parse]` and nested tables

| Key | Type | Default | CLI override | Description |
| --- | --- | --- | --- | --- |
| `parse.extensions.gfm.autolinks` | "disabled" \| "urls" \| "urls-and-emails" | `"urls-and-emails"` | `none` | Recognise GFM bare URL and email autolinks as document facts and render them as links. Use `urls` to leave bare emails as text or `disabled` for strict CommonMark-style text treatment. |
| `parse.extensions.gfm.tagfilter` | bool | `true` | `none` | Apply GFM tagfiltering when rendering or building semantic signatures. This escapes the raw HTML tags that cmark-gfm filters, without rewriting source bytes. |
| `parse.extensions.definition-lists` | bool | `true` | `none` | Recognise `Term\n: definition\n` definition lists. Default on; turn off on non-mkdocs corpora to suppress recognition. |
| `parse.extensions.abbreviation-lists` | bool | `true` | `none` | Recognise `*[ABBR]: definition` abbreviation declarations as a scan-and-preserve overlay. mdwright does not expand occurrences; the downstream renderer does. |
| `parse.extensions.heading-attribute-lists` | bool | `true` | `none` | Recognise `# Heading {#id .class}` trailers via pulldown's `ENABLE_HEADING_ATTRIBUTES`. When off, the trailer reads as plain text in the heading body. |
| `parse.extensions.block-attribute-lists` | bool | `true` | `none` | Recognise `{ .class }` on a line by itself after a non-empty block as a scan-and-preserve overlay. Inline attribute lists (mid-paragraph) are out of scope. |
| `parse.extensions.myst.directive-containers` | bool | `true` | `none` | Recognise MyST `:::{name}` directive containers (with `:KEY: value` options) as a scan-and-preserve overlay. mdwright does not expand directives; downstream renderers (Sphinx, jupyter-book) do. |
| `parse.extensions.myst.inline-roles` | bool | `true` | `none` | Recognise MyST `` {role}`payload` `` inline roles as a scan-and-preserve overlay inside paragraph text. |
| `parse.extensions.myst.substitution-references` | bool | `true` | `none` | Recognise MyST `{{name}}` inline substitution references as a scan-and-preserve overlay. Declarations live in YAML frontmatter under `myst_substitutions:` and round-trip through the frontmatter verbatim path. |
| `parse.extensions.myst.comments` | bool | `true` | `none` | Recognise MyST `%` line comments at line-start as a scan-and-preserve overlay. |
| `parse.extensions.pandoc.fenced-divs` | bool | `true` | `none` | Recognise Pandoc `::: {.cls}` fenced div openers (attribute form). Closer is a colon-only line of matching count. |
| `parse.extensions.pandoc.short-form-divs` | bool | `true` | `none` | Recognise Pandoc `:::name` fenced div openers (short form). |
| `parse.extensions.pandoc.inline-attribute-spans` | bool | `true` | `none` | Recognise Pandoc `[content]{.cls}` inline attribute spans as a scan-and-preserve overlay. |

<!-- END GENERATED -->
