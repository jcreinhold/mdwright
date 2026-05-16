# Known fuzz-found issues (deferred)

Inputs in this directory crash one of the fuzz harnesses but have
**not** been fixed yet — their fix has tradeoffs (GFM-spec regressions,
behaviour changes) that need design discussion before they land. They
are kept here so the fuzzer's seed corpus carries the minimised
reproducer; do not add them to `tests/regressions/` (the regression
suite would fail).

When you fix one, move the file to `tests/regressions/fuzz_<hash>.in`
(or `fuzz_<hash>.idem.in` if the fix is idempotence-only — see
`tests/regressions.rs`) and delete the matching entry from this
README.

## idempotence-bullet-marker-rewrite.in

Bytes (28): `* \v \n \0 \0 \n \n \0 \0 \0 \0 \x04 \0 \0 \0 \0 \0 \0 \0 \x17 \x17 \x17 \n \v \0 \0 > * \n` (the leading `*\v` is a `*` followed by a vertical tab).

Once preserves the leading `*` line; twice rewrites it as a bullet
list item (`- `) and inserts a leading blank line, perturbing the
remaining structure:

```
once : *\n\0\0\n\n\0\0\0\0\x04\0\0\0\0\0\0\0\x17\x17\x17\n\v\0\0>*\n
twice: - \n\n\0\0\n\n\0\0\0\0\x04\0\0\0\0\0\0\0\x17\x17\x17\n\v\0\0>*\n
```

Bug class: a paragraph whose first line is exactly one `*` (after
verbatim emission strips a trailing `\v` from its source line)
reparses as the marker of an empty bullet list item. Sibling of
the paragraph-line-start escape family, but the trigger is a
*first*-line `*` (not a continuation line). `ParagraphBody`'s
line-start safety pass currently restricts itself to continuation
lines.

Fix shape (to investigate): extend the line-start safety pass to
the first line as well, OR refuse root-verbatim emission for
paragraphs whose first source line collapses to a single bullet-
marker character after the chokepoint LF-norm.
