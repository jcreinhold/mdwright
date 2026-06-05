---
name: table-pipe-spacing
default: true
advisory: false
fix: true
since: unreleased
---

# table-pipe-spacing

Table cell separator with no space before it, dropping the row's column alignment.

A GFM table cell separator (`|`) that is not preceded by whitespace
makes the renderer drop the **whole row's column alignment**. In a
table with an explicit `:--`, `--:`, or `:-:` column, the affected row
stops emitting its `text-align`, so the column renders ragged even
though the delimiter row asks for alignment.

For example, the middle separator below abuts the first cell's content:

```markdown
| File | Words |
| --- | ---: |
| report.md| 1.7k |
```

The `Words` column is declared right-aligned, but because `report.md|`
has no space before the pipe, that row renders left-aligned.

The same source shape blocks `mdwright fmt` under `[fmt.tables] style =
"align"`: padding the cell to align the column would insert the missing
space, which changes how the table parses, so the formatter preserves
the table untouched rather than alter the rendered output. One such row
prevents the table from ever being aligned.

The fix is to put a space before the separator:

```markdown
| File | Words |
| --- | ---: |
| report.md | 1.7k |
```

The delimiter row (`| --- | ---: |`) is exempt: it carries no content
and renders correctly even when fully compact (`|---|---:|`).
