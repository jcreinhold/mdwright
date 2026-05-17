# Diagnostic schema

`mdwright check --format=json` emits one JSON object per line (JSON Lines), one object per
diagnostic. The current schema is version **2**, defined formally by
[`diagnostic-schema.json`](diagnostic-schema.json) (JSON Schema draft 2020-12).

The v1 schema remains available under `--format=json-v1` for one release cycle and emits a
deprecation warning to stderr.

## Example record (pretty-printed)

```json
{
  "schema_version": 2,
  "path": "docs/note.md",
  "severity": "error",
  "rule": {
    "name": "math/unbalanced-delim",
    "description": "TeX-style math open delimiter (`\\[`, `\\(`, `$$`, `$`) with no matching close.",
    "url": "docs/rules/math/unbalanced-delim.md"
  },
  "source": {
    "line": 42,
    "column": 10,
    "span_start": 1037,
    "span_end": 1039,
    "snippet": "text with \\[ unmatched math"
  },
  "message": "no matching `\\]` before end of document",
  "fix": null
}
```

In the wire format each record is a single line terminated by `\n`; the `pretty-printed` form
above is for human reading only.

## Field reference

### Top-level

| Field            | Type              | Required | Notes                                                                    |
| ---------------- | ----------------- | -------- | ------------------------------------------------------------------------ |
| `schema_version` | integer (`2`)     | yes      | Bumped on incompatible schema changes.                                   |
| `path`           | string            | yes      | File path as given on the CLI; `<stdin>` for piped input.                |
| `severity`       | enum              | yes      | `error`, `warning`, or `advisory`. `warning` is reserved for future use. |
| `rule`           | object            | yes      | See below.                                                               |
| `source`         | object            | yes      | See below.                                                               |
| `message`        | string            | yes      | Single-sentence description of the problem.                              |
| `fix`            | object \| omitted | no       | Present when a replacement is suggested.                                 |

### `rule`

| Field         | Type   | Notes                                                                                                       |
| ------------- | ------ | ----------------------------------------------------------------------------------------------------------- |
| `name`        | string | Kebab-case identifier (e.g. `bare-url`, `math/unbalanced-delim`).                                           |
| `description` | string | One-line summary; same text as `mdwright list-rules`.                                                       |
| `url`         | string | Repository-relative path into `docs/rules/`. Will become an absolute URL once the mdBook site is published. |

### `source`

| Field        | Type          | Notes                                                                                                                                        |
| ------------ | ------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `line`       | integer (≥ 1) | 1-indexed line of the diagnostic's first byte.                                                                                               |
| `column`     | integer (≥ 1) | 1-indexed codepoint column.                                                                                                                  |
| `span_start` | integer (≥ 0) | Byte offset of the first byte of the offending region.                                                                                       |
| `span_end`   | integer (≥ 0) | Byte offset one past the last byte.                                                                                                          |
| `snippet`    | string        | The source line, with trailing newline stripped. Multi-line spans are clipped to the first line — the caret region still starts at `column`. |

### `fix`

| Field         | Type    | Notes                                                  |
| ------------- | ------- | ------------------------------------------------------ |
| `replacement` | string  | Text to substitute for `[span_start, span_end)`.       |
| `safe`        | boolean | `mdwright fix` only applies fixes with `"safe": true`. |

## Lifecycle

- v2 is the current default. v1 remains available under `--format=json-v1` for one release
  cycle and is then removed.
- New schema versions bump `schema_version` and ship alongside the previous version for at
  least one cycle.

## Validation

The schema is JSON Schema draft 2020-12 (`$schema` field in
[`diagnostic-schema.json`](diagnostic-schema.json)). Any draft 2020-12-compatible validator
(`jsonschema` Python package, `ajv` for JavaScript, etc.) can validate output records against
it.
