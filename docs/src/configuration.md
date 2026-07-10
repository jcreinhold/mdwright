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

Run `mdwright config init` to create a documented `.mdwright.toml`
starter file with every option set to its default.

## Single-file integration via `pyproject.toml`

For projects that already use `pyproject.toml`, the entire mdwright
configuration can live there under `[tool.mdwright]`:

```toml
# pyproject.toml
[tool.mdwright]
lint.preset = "default"
lint.extend-select = ["latex-command"]

[tool.mdwright.fmt]
wrap = 100
```

## CLI overrides

The following knobs accept CLI flags that take precedence over the
config file:

- `lint.preset`, `lint.select`, `lint.extend-select`, `lint.ignore`: `--rules`
- `render.profile`: `mdwright render --render-profile`
- `--no-suppress` toggles whether `<!-- mdwright: allow ... -->`
  comments are honoured; there is no config-file equivalent.

All `[fmt]` knobs are config-file-only.

## Schema reference

<!-- BEGIN GENERATED: do not edit. Regenerate by running `cargo xtask doc-config`. -->

### `[lint]` and nested tables

| Key | Type | Default | CLI override | Description |
| --- | --- | --- | --- | --- |
| `lint.preset` | "default" \| "all" \| "none" | `"default"` | `--rules` | Baseline lint rule set. Use `default` for curated defaults, `all` for every registered rule, or `none` with `lint.select` for an explicit set. |
| `lint.select` | array of string | `[]` | `--rules` | Exact lint rule names to enable when `lint.preset = "none"`. Preset names are not valid rule names here. |
| `lint.extend-select` | array of string | `[]` | `--rules` | Lint rule names to add on top of `lint.preset`. |
| `lint.ignore` | array of string | `[]` | `--rules` | Lint rule names to remove after applying `lint.preset`, `lint.select`, and `lint.extend-select`. |
| `lint.exclude` | array of string | `[]` | `none` | Gitignore-style patterns. Matching files are dropped from lint runs. Patterns are anchored to the directory containing the config file. |
| `lint.info-strings.extra` | array of string | `[]` | `none` | Project-specific additions to the `info-string-typo` allowlist. The stdlib default allowlist still applies. |
| `lint.render.renderer` | "mathjax-v3" \| "katex" | `"mathjax-v3"` | `none` | Math renderer the `math/render-compat` rule should check against. |
| `lint.render.packages` | array of string | `[]` | `none` | Renderer packages/extensions to load on top of the renderer's default autoload set (e.g. `["mhchem", "physics"]`). Consumed by the `math/render-compat` rule. |
| `lint.render.macros` | table | `{}` | `none` | User-declared macros known to be in scope, keyed by command name (no leading backslash). Each entry is either `name = arity` or `name = { arity = N }`. |

### `[fmt]` and nested tables

| Key | Type | Default | CLI override | Description |
| --- | --- | --- | --- | --- |
| `fmt.profile` | "preserve" \| "mdformat" | `"preserve"` | `none` | Formatter style profile. `preserve` keeps mdwright's identity-oriented defaults; `mdformat` applies mdformat-compatible defaults where verified rewrites can preserve semantics. Explicit `[fmt]` keys override profile defaults. |
| `fmt.wrap` | "keep" \| "no" \| int | `"keep"` | `none` | Wrap mode for prose paragraphs. `keep` leaves existing breaks alone; `no` forbids new breaks; an integer enforces that display-column budget for breakable lines in every formatter profile. |
| `fmt.wrap-strategy` | "stable" \| "balanced" | `"stable"` | `none` | Reflow strategy used when `fmt.wrap` is an integer. `stable` greedily fills soft-break runs and is the default; `balanced` rebalances paragraphs for more even line lengths. |
| `fmt.italic` | "asterisk" \| "underscore" \| "preserve" | `"preserve"` | `none` | Italic delimiter canonicalisation. `preserve` leaves source bytes; `asterisk` or `underscore` opts into the post-pass rewrite. See [Style knobs](format/style.md). |
| `fmt.strong` | "asterisk" \| "underscore" \| "preserve" | `"preserve"` | `none` | Strong-emphasis delimiter canonicalisation. Independent of `fmt.italic`: `*italic*` with `__strong__` is expressible. |
| `fmt.list-marker` | "dash" \| "asterisk" \| "plus" \| "preserve" | `"preserve"` | `none` | Unordered-list bullet canonicalisation. Each marker is rewritten through a marker-local fact and the family commits only after verification. |
| `fmt.ordered-list` | "one" \| "consistent" \| "preserve" | `"preserve"` | `none` | Ordered-list number canonicalisation. `one` rewrites markers to `1.` only when verification preserves the list start; `consistent` renumbers each list from the source's first item; `preserve` keeps source numbering verbatim. |
| `fmt.thematic-break` | "dash" \| "asterisk" \| "underscore" \| "underscore-70" \| "preserve" | `"preserve"` | `none` | Thematic-break canonicalisation. Fixed character modes preserve the source repeat count and spacing; `underscore-70` rewrites the whole break line to mdformat's 70 underscores. |
| `fmt.trailing-newline` | "preserve" \| "strip" \| "ensure" \| bool | `"preserve"` | `none` | Trailing-newline policy at the document boundary. `true` is accepted as a synonym for `ensure` and `false` for `strip`. |
| `fmt.end-of-line` | "lf" \| "crlf" \| "keep" | `"lf"` | `none` | Line-ending normalisation. `keep` adopts the first newline seen in the source. |
| `fmt.exclude` | array of string | `[]` | `none` | Formatter-specific exclude globs, independent of `[lint] exclude`. |
| `fmt.heading-attrs` | "preserve" \| "canonicalise" | `"preserve"` | `none` | ATX heading `{#id .class key=val}` trailer emission. `preserve` emits the source trailer byte-verbatim. `canonicalise` emits id first, then classes, then key-value pairs. |
| `fmt.blank-line-before-heading` | "preserve" \| "one" | `"preserve"` | `none` | Blank lines before a top-level heading. `preserve` echoes the source. `one` emits exactly one blank line, inserting or collapsing as needed. A heading that opens the document has nothing before it and is left alone. |
| `fmt.blank-line-after-heading` | "preserve" \| "one" | `"preserve"` | `none` | Blank lines between a top-level heading and the block that follows it. `preserve` echoes the source. `one` emits exactly one blank line, inserting or collapsing as needed. |
| `fmt.refs.placement` | "end" \| "preserve" | `"end"` | `none` | Where reference-link definitions are emitted: gathered and sorted at the end of the document, or kept in source order. |
| `fmt.refs.style` | "bare" \| "angle" \| "preserve" | `"preserve"` | `none` | Destination style for reference-link and inline-link URLs. `preserve` keeps each destination's source form; `bare` strips wrapping `<...>` where the bare form still parses; `angle` wraps every destination in `<...>`. |
| `fmt.footnotes.placement` | "end" \| "preserve" | `"preserve"` | `none` | Where footnote definitions are emitted. Default is `preserve` because pulldown-cmark's HTML renderer ties footnote position to parse order; moving definitions would change the rendered HTML. |
| `fmt.tables.style` | "compact" \| "align" \| "preserve" | `"compact"` | `none` | GFM table spacing policy. `compact` trims cell padding to one space on each side; `align` pads columns by display width; `preserve` keeps source cell spacing. |
| `fmt.lists.continuation-indent` | "marker-width" \| "four-space" | `"marker-width"` | `none` | Continuation indentation for wrapped list-item paragraphs. `marker-width` aligns to the source marker width; `four-space` matches mdformat's list continuation spelling. |
| `fmt.frontmatter.preserve` | bool | `true` | `none` | Whether to emit document frontmatter byte-verbatim. `false` strips it. |
| `fmt.math.normalise` | bool | `false` | `none` | Whether whole-block math regions are normalised. Off by default because math bytes are opaque to CommonMark. |
| `fmt.math.render` | "none" \| "commonmark-katex" \| "dollar" | `"none"` | `none` | Math delimiter rendering policy for downstream renderers. `none` preserves source math regions; `commonmark-katex` records intent without rewriting; `dollar` rewrites bracket and paren math to dollar delimiters. |

### `[parse]` and nested tables

| Key | Type | Default | CLI override | Description |
| --- | --- | --- | --- | --- |
| `parse.math.delimiters` | "tex" \| "github" | `"tex"` | `none` | Math delimiter recognition policy. `tex` recognises `\(...\)`, `\[...\]`, and LaTeX environments; `github` also recognises `$...$` and `$$...$$`. |
| `parse.extensions.definition-lists` | bool | `true` | `none` | Recognise `Term\n: definition\n` definition lists. Turn off on non-mkdocs corpora to suppress recognition. |
| `parse.extensions.abbreviation-lists` | bool | `true` | `none` | Recognise `*[ABBR]: definition` abbreviation declarations as a scan-and-preserve overlay. mdwright does not expand occurrences; the downstream renderer does. |
| `parse.extensions.heading-attribute-lists` | bool | `true` | `none` | Recognise `# Heading {#id .class}` trailers via pulldown's heading-attribute extension. When off, the trailer reads as plain text in the heading body. |
| `parse.extensions.block-attribute-lists` | bool | `true` | `none` | Recognise `{ .class }` on a line by itself after a non-empty block as a scan-and-preserve overlay. Inline attribute lists are out of scope. |
| `parse.extensions.gfm.autolinks` | "disabled" \| "urls" \| "urls-and-emails" | `"urls-and-emails"` | `none` | Recognise GFM bare URL and email autolinks as document facts and render them as links. Use `urls` to leave bare emails as text or `disabled` for strict CommonMark-style text treatment. |
| `parse.extensions.gfm.tagfilter` | bool | `true` | `none` | Apply GFM tagfiltering when rendering or building semantic signatures. This escapes the raw HTML tags that cmark-gfm filters, without rewriting source bytes. |
| `parse.extensions.myst.directive-containers` | bool | `true` | `none` | Recognise MyST `:::{name}` directive containers with `:KEY: value` options as a scan-and-preserve overlay. mdwright does not expand directives; downstream renderers do. |
| `parse.extensions.myst.inline-roles` | bool | `true` | `none` | Recognise MyST `` {role}`payload` `` inline roles as a scan-and-preserve overlay inside paragraph text. |
| `parse.extensions.myst.substitution-references` | bool | `true` | `none` | Recognise MyST `{{name}}` inline substitution references as a scan-and-preserve overlay. Declarations live in YAML frontmatter and round-trip through the frontmatter path. |
| `parse.extensions.myst.comments` | bool | `true` | `none` | Recognise MyST `%` line comments at line-start as a scan-and-preserve overlay. |
| `parse.extensions.pandoc.fenced-divs` | bool | `true` | `none` | Recognise Pandoc `::: {.cls}` fenced div openers. The closer is a colon-only line of matching count. |
| `parse.extensions.pandoc.short-form-divs` | bool | `true` | `none` | Recognise Pandoc `:::name` fenced div openers. |
| `parse.extensions.pandoc.inline-attribute-spans` | bool | `true` | `none` | Recognise Pandoc `[content]{.cls}` inline attribute spans as a scan-and-preserve overlay. |

### `[render]` and nested tables

| Key | Type | Default | CLI override | Description |
| --- | --- | --- | --- | --- |
| `render.profile` | "pulldown" \| "cmark-gfm" | `"pulldown"` | `--render-profile` | HTML spelling profile for `mdwright render`. `pulldown` preserves the default renderer; `cmark-gfm` matches cmark-gfm spelling where parser semantics already agree. |

<!-- END GENERATED -->
