# Style knobs

This page documents each style knob in `[fmt]`. Every knob defaults to `"preserve"`,
which means the canonicalisation pass leaves source bytes unchanged for that construct.
Set a non-preserve value to opt into rewriting.

See [Formatter policy](policy.md) for the overall design (structural emit + opt-in
canonicalisation) and [Configuration](../configuration.md) for the full
`.mdwright.toml` schema.

## `[fmt] italic`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Emphasis delimiters round-trip from source. `_foo_` stays `_foo_`; `*foo*` stays `*foo*`. |
| `"asterisk"` | Rewrite `_…_` to `*…*` when verification preserves the parse. |
| `"underscore"` | Rewrite `*…*` to `_…_` when verification preserves the parse. |

**Verification skips when:** the rewrite would change the parse of the enclosing paragraph
window. The most common case is intraword underscore (`id_S`, `Hom_{cart}`): pulldown
already treats these as plain text under CM §6.2 rule 6, so no rewrite is proposed and
nothing skips. Where rewrites *do* skip silently is in dense multi-delimiter runs
(`*_*…*_*`-style chains) whose pairing depends on flanking neighbours; verification
catches these and leaves the source bytes in place.

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

Independent of `italic`. With `italic = "asterisk"` and `strong = "underscore"` you get
`*italic*` alongside `__strong__`. `italic` and `strong` are independent knobs.

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

**Atomic per list.** The pass rewrites every bullet in a list together, not bullet by
bullet. Partial rewrites would split the list at the parse layer (mixed-marker lists are
two adjacent lists in pulldown's view). Nested lists are treated independently; the
outer list and inner list each commit or skip.

```toml
[fmt]
list-marker = "dash"
```

## `[fmt] ordered-list`

| Value | Effect |
|---|---|
| `"preserve"` (default) | Each ordered list keeps its source numbering. `3. a / 5. b / 9. c` stays. |
| `"consistent"` | Renumber so item `k` (0-indexed) becomes `start_num + k`, where `start_num` is the source's first item's number. `3. a / 5. b / 9. c` → `3. a / 4. b / 5. c`. |

Atomic per list: every marker in the list updates together, or none does. The starting
number is preserved (mirrors mdformat's behaviour); only the increment is canonicalised.

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

The repeat count and internal spacing are preserved; only the character changes. So
`* * *` becomes `_ _ _` under `"underscore"`, not `___`.

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

Applies to both reference-link definitions (`[ref]: dest`) and inline link destinations
(`[text](dest)`). Verification skips when the bare form contains whitespace, unbalanced
parentheses, or other bytes that would prevent pulldown from parsing it as a bare
destination; the angle-wrapped form is kept in those cases.

```toml
[fmt.refs]
style = "angle"
```

## Combined example

```toml
[fmt]
italic = "asterisk"
strong = "asterisk"
list-marker = "dash"
thematic-break = "dash"
ordered-list = "consistent"

[fmt.refs]
style = "angle"
```

Approximates mdformat's default style on a per-knob basis. The fuzz harness exercises
exactly this combination as one of its 16 enumerated modes.

## How verification skips become visible

When a rewrite would change the parse of the enclosing paragraph window, the
canonicalisation pass logs a `tracing::warn!` with the byte span and skipped rewrite.
Capture these in production with `RUST_LOG=mdwright_format=warn`. A high skip
rate on one document usually points at a structural-emit edge case worth filing as a
regression input.
