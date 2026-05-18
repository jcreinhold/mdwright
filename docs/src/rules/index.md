# Lint rules

Every rule shipped by mdwright's standard library. Each link points to the rule's long-form
explanation; `mdwright explain <name>` prints the same text from the command line.

| Rule | Default | Advisory | Fix | Description |
| --- | --- | --- | --- | --- |
| [`unbalanced-backtick`](unbalanced-backtick.md) | yes | no | no | Backtick in prose that could not be paired with a closing fence. |
| [`math/unbalanced-delim`](math/unbalanced-delim.md) | yes | no | no | TeX-style math open delimiter (`\[`, `\(`, `$$`, `$`) with no matching close. |
| [`math/unbalanced-env`](math/unbalanced-env.md) | yes | no | no | LaTeX `\begin{env}` with no matching `\end{env}` at the same nesting depth. |
| [`math/unbalanced-braces`](math/unbalanced-braces.md) | yes | no | no | `{` / `}` inside a math body do not balance; math body normalisation is skipped for that region. |
| [`adjacent-code-no-space`](adjacent-code-no-space.md) | yes | no | no | Inline code span adjacent to a letter without whitespace. |
| [`heading-punctuation`](heading-punctuation.md) | yes | no | no | Trailing `.` or `:` on a heading. |
| [`orphan-reference-link`](orphan-reference-link.md) | yes | no | no | Reference-style link with no matching `[label]:` definition. |
| [`duplicate-link-label`](duplicate-link-label.md) | yes | no | no | Two `[label]:` definitions with the same label. |
| [`bare-url`](bare-url.md) | yes | no | yes | Bare URL in prose; wrap in `<…>` for a CommonMark autolink. |
| [`trailing-whitespace`](trailing-whitespace.md) | yes | no | yes | Trailing whitespace at end of line. |
| [`inconsistent-list-marker`](inconsistent-list-marker.md) | yes | no | no | Mixed `-` / `*` / `+` markers in one bullet list. |
| [`list-tightness-flipped`](list-tightness-flipped.md) | no | yes | no | list tightness from the tree disagrees with tightness from source bytes |
| [`duplicate-heading`](duplicate-heading.md) | yes | yes | no | Two headings at the same level under the same parent with the same text. |
| [`unicodeable-subscript`](unicodeable-subscript.md) | yes | yes | yes | Braced super/subscript that has a single-codepoint Unicode form. |
| [`info-string-typo`](info-string-typo.md) | yes | yes | no | Fenced code block info string not in the known-languages allowlist. |
| [`stray-dollar`](stray-dollar.md) | no | no | yes | Literal `$` in prose (opt-in for projects that don't use $…$ math). |
| [`latex-command`](latex-command.md) | no | no | yes | LaTeX control sequence in prose (opt-in for Unicode-math projects). |
| [`escaped-emphasis`](escaped-emphasis.md) | no | no | yes | Literal `\_`, `\*`, or `` \` `` escape in prose (mdformat damage). |
| [`subscript-damage`](subscript-damage.md) | no | no | yes | Identifier with `*` where a `_` subscript was expected (formatter damage). |
