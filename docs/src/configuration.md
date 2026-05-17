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

- `lint.rules` — `--rules`
- (formatter mode is exposed via `--mode` but is not currently a
  config-file knob)
- `--no-suppress` toggles whether `<!-- mdwright: allow ... -->`
  comments are honoured; there is no config-file equivalent.

All other `[fmt]` knobs are config-file-only.

## Schema reference

<!-- BEGIN GENERATED — do not edit. Regenerate by running `cargo build` after editing `build.rs`. -->

### `[lint]` and nested tables

| Key                       | Type            | Default     | CLI override | Description                                                                                                                             |
| ------------------------- | --------------- | ----------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| `lint.rules`              | string          | `"default"` | `--rules`    | Rule-selection spec. Comma-separated tokens: `all`, `default`, `<name>` (start from `{<name>}`), `+<name>` (add), `-<name>` (remove).   |
| `lint.exclude`            | array of string | `[]`        | `—`          | Gitignore-style patterns. Matching files are dropped from lint runs. Patterns are anchored to the directory containing the config file. |
| `lint.info-strings.extra` | array of string | `[]`        | `—`          | Project-specific additions to the `info-string-typo` allowlist. The stdlib's default allowlist still applies.                           |

### `[fmt]` and nested tables

| Key                        | Type                                         | Default        | CLI override | Description                                                                                                                                                                                    |
| -------------------------- | -------------------------------------------- | -------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fmt.wrap`                 | "keep" \| "no" \| int                        | `"keep"`       | `—`          | Wrap mode for prose paragraphs. `keep` leaves existing breaks alone; `no` forbids new breaks; an integer wraps at that column.                                                                 |
| `fmt.italic`               | "asterisk" \| "underscore" \| "preserve"     | `"asterisk"`   | `—`          | Italic delimiter normalisation policy.                                                                                                                                                         |
| `fmt.list-marker`          | "dash" \| "asterisk" \| "plus" \| "preserve" | `"dash"`       | `—`          | Unordered-list bullet normalisation.                                                                                                                                                           |
| `fmt.ordered-list`         | "consistent" \| "preserve"                   | `"consistent"` | `—`          | Ordered-list number normalisation. `consistent` renumbers from 1; `preserve` keeps the source numbering verbatim.                                                                              |
| `fmt.trailing-newline`     | "preserve" \| "strip" \| "ensure" \| bool    | `"preserve"`   | `—`          | Trailing-newline policy at the document boundary. `true` is accepted as a synonym for `ensure` and `false` for `strip` (legacy schema).                                                        |
| `fmt.end-of-line`          | "lf" \| "crlf" \| "keep"                     | `"lf"`         | `—`          | Line-ending normalisation. `keep` adopts the first newline seen in the source.                                                                                                                 |
| `fmt.exclude`              | array of string                              | `[]`           | `—`          | Formatter-specific exclude globs, independent of `[lint] exclude`.                                                                                                                             |
| `fmt.refs.placement`       | "end" \| "preserve"                          | `"end"`        | `—`          | Where reference-link definitions are emitted: gathered and sorted at the end of the document, or kept in source order.                                                                         |
| `fmt.refs.style`           | "bare" \| "angle"                            | `"bare"`       | `—`          | Destination style for reference-link and inline-link URLs.                                                                                                                                     |
| `fmt.footnotes.placement`  | "end" \| "preserve"                          | `"preserve"`   | `—`          | Where footnote definitions are emitted. Default is `preserve` because pulldown-cmark's HTML renderer ties footnote position to parse order; moving definitions would change the rendered HTML. |
| `fmt.frontmatter.preserve` | bool                                         | `true`         | `—`          | Whether to emit document frontmatter byte-verbatim. `false` strips it.                                                                                                                         |

<!-- END GENERATED -->
