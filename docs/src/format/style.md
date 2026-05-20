# Style knobs

This page documents each style knob in `[fmt]`. Every knob defaults to `"preserve"`, which means the canonicalisation
pass leaves source bytes unchanged for that construct. Set a non-preserve value to opt into rewriting.

See [Formatter policy](policy.md) for the overall design (structural emit + opt-in canonicalisation) and
[Configuration](../configuration.md) for the full `.mdwright.toml` schema.

## `[fmt] italic`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Emphasis delimiters round-trip from source. `_foo_` stays `_foo_`; `*foo*` stays `*foo*`. |
| `"asterisk"` | Rewrite `_…_` to `*…*` when verification preserves the parse. |
| `"underscore"` | Rewrite `*…*` to `_…_` when verification preserves the parse. |

**Verification skips when:** the rewrite would change the parse of the enclosing paragraph window. The most common
case is intraword underscore (`id_S`, `Hom_{cart}`): pulldown already treats these as plain text under CM §6.2 rule
6, so no rewrite is proposed and nothing skips. Where rewrites *do* skip silently is in dense multi-delimiter runs
(`*_*…*_*`-style chains) whose pairing depends on flanking neighbours; verification catches these and leaves the source
bytes in place.

```toml
[fmt]
italic = "asterisk"
```

## `[fmt] strong`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Strong delimiters round-trip from source. `**foo**` stays `**foo**`; `__foo__` stays `__foo__`. |
| `"asterisk"` | Rewrite `__…__` to `**…**`. |
| `"underscore"` | Rewrite `**…**` to `__…__`. |

Independent of `italic`. With `italic = "asterisk"` and `strong = "underscore"` you get `*italic*` alongside
`__strong__`. `italic` and `strong` are independent knobs.

```toml
[fmt]
italic = "asterisk"
strong = "underscore"
```

## `[fmt] list-marker`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Each unordered list keeps its source bullet character. |
| `"dash"` | Rewrite each bullet to `-`. |
| `"asterisk"` | Rewrite each bullet to `*`. |
| `"plus"` | Rewrite each bullet to `+`. |

**Marker-local.** The document crate exposes one fact per list-item marker. The formatter rewrites those marker bytes
only, then verifies the full document before committing the family plan. Nested list markers are separate facts, so an
outer list rewrite cannot cover child markers accidentally.

```toml
[fmt]
list-marker = "dash"
```

## `[fmt] ordered-list`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Each ordered list keeps its source numbering. `3. a / 5. b / 9. c` stays. |
| `"one"` | Rewrite markers to `1.` when verification preserves the list start. This matches mdformat's default spelling for ordinary lists that already start at `1.`. |
| `"consistent"` | Renumber so item `k` (0-indexed) becomes `start_num + k`, where `start_num` is the source's first item's number. `3. a / 5. b / 9. c` → `3. a / 4. b / 5. c`. |

Marker-local: each ordered item exposes its digit range, list start, and ordinal. The family plan rewrites those digit
ranges and commits only after full-document verification. The starting number is preserved; only the increment is
canonicalised.

```toml
[fmt]
ordered-list = "consistent"
```

## `[fmt] thematic-break`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Thematic breaks keep their source character (`---`, `***`, `___`). |
| `"dash"` | Rewrite to `---`. |
| `"asterisk"` | Rewrite to `***`. |
| `"underscore"` | Rewrite to `___`. |
| `"underscore-70"` | Rewrite the whole line to 70 underscores, matching mdformat's default thematic-break spelling. |

The repeat count and internal spacing are preserved; only the character changes. So `* * *` becomes `_ _ _` under
`"underscore"`, not `___`. Use `"underscore-70"` when you want the mdformat spelling.

```toml
[fmt]
thematic-break = "dash"
```

## `[fmt.refs] style`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Each link destination keeps its source form: `[ref]: url` or `[ref]: <url>` survives. |
| `"bare"` | Strip angle brackets where the bare form would still parse. `[ref]: <url>` → `[ref]: url`. |
| `"angle"` | Wrap destinations in angle brackets. `[ref]: url` → `[ref]: <url>`. |

Applies to both reference-link definitions (`[ref]: dest`) and inline link destinations (`[text](dest)`). Verification
skips when the bare form contains whitespace, unbalanced parentheses, or other bytes that would prevent pulldown from
parsing it as a bare destination; the angle-wrapped form is kept in those cases.

```toml
[fmt.refs]
style = "angle"
```

## `[fmt.tables] style`

| Value | Effect |
|---|---|
| `"preserve"` (default) | GFM table spacing round-trips from source. |
| `"pad"` | Pad cells and delimiter rows to mdformat-compatible widths when verification preserves the parse. |

Padding is a table-level operation. Inline delimiter and link destination rewrites run first; table padding then reads
the current cell bytes and rewrites the table block as one verified replacement. Tables with source cells the document
facts cannot account for are left unchanged rather than partially rewritten.

```toml
[fmt.tables]
style = "pad"
```

## `[fmt] wrap`

| Value | Effect |
|---|---|
| `"keep"` (default) | Preserve existing paragraph line breaks. |
| `"no"` | Collapse soft line breaks inside paragraphs where verification preserves the parse. |
| integer | Wrap breakable prose lines at that display-column width. |

`wrap = 120` means breakable output lines should fit within 120 columns in every formatter profile. The accepted
exception is an indivisible atomic token, such as a long code span, URL, math atom, or single long word. Those tokens
are left intact rather than split into invalid Markdown.

The default profile uses mdwright's balanced paragraph planner for explicit wrapping. The mdformat profile treats
ordinary source newlines inside a paragraph as soft breaks, reflows each hard-break-bounded run, and uses the same final
line-budget check.

```toml
[fmt]
wrap = 120
```

## `[fmt.lists] continuation-indent`

| Value | Effect |
|---|---|
| `"marker-width"` (default) | Continuation lines align under the source list marker width. |
| `"four-space"` | Continuation lines use four spaces after the containing block prefix. |

This setting only affects paragraphs that are wrapped inside list items. It is separate from `list-marker` because the
bullet character and continuation indentation are independent style decisions. The mdformat profile defaults this key
to `"four-space"`; explicit config overrides that default.

```toml
[fmt]
wrap = 120

[fmt.lists]
continuation-indent = "four-space"
```

## Combined example

```toml
[fmt]
profile = "mdformat"
```

This keeps mdformat's default `wrap = keep`, sets list continuation indentation to four spaces, and applies mdformat
spelling for supported style knobs. Explicit keys override the profile:

```toml
[fmt]
profile = "mdformat"
wrap = 120

[fmt.lists]
continuation-indent = "marker-width"
```

A per-knob spelling can also be written without the profile:

```toml
[fmt]
list-marker = "dash"
thematic-break = "underscore-70"
ordered-list = "one"

[fmt.refs]
style = "angle"

[fmt.tables]
style = "pad"
```

This is mdformat-compatible where mdwright has verified rewrite support. It does not move orphan footnotes or copy
mdformat behaviours that would change the parsed document.

## How verification skips become visible

When a rewrite would change the parse of the enclosing paragraph window, the canonicalisation pass
logs a `tracing::warn!` with the byte span and skipped rewrite. Capture these in production with
`RUST_LOG=mdwright_format=warn`. A high skip rate on one document usually points at a structural-emit edge case worth
filing as a regression input.
