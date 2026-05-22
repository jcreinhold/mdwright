# Lint rules

Every rule shipped by mdwright's standard library, grouped by how they behave on
a fresh install. Each link points to the rule's long-form explanation;
`mdwright explain <name>` prints the same text from the command line.

## Default rules

On by default. A diagnostic from one of these fails `mdwright check --check`.

| Rule | Fix | Description |
| --- | --- | --- |
| [`unbalanced-backtick`](unbalanced-backtick.md) | no | Backtick in prose that could not be paired with a closing fence. |
| [`math/unbalanced-delim`](math/unbalanced-delim.md) | no | TeX-style math open delimiter (`\[`, `\(`, `$$`, `$`) with no matching close. |
| [`math/unbalanced-env`](math/unbalanced-env.md) | no | LaTeX `\begin{env}` with no matching `\end{env}` at the same nesting depth. |
| [`math/unbalanced-braces`](math/unbalanced-braces.md) | no | `{` / `}` inside a math body do not balance; math body normalisation is skipped for that region. |
| [`adjacent-code-no-space`](adjacent-code-no-space.md) | yes | Inline code span adjacent to a letter without whitespace. |
| [`heading-punctuation`](heading-punctuation.md) | yes | Trailing `.` or `:` on a heading. |
| [`orphan-reference-link`](orphan-reference-link.md) | no | Reference-style link with no matching `[label]:` definition. |
| [`duplicate-link-label`](duplicate-link-label.md) | no | Two `[label]:` definitions with the same label. |
| [`bare-url`](bare-url.md) | yes | Bare URL in prose; wrap in `<…>` for a CommonMark autolink. |
| [`trailing-whitespace`](trailing-whitespace.md) | yes | Trailing whitespace at end of line. |
| [`inconsistent-list-marker`](inconsistent-list-marker.md) | yes | Mixed `-` / `*` / `+` markers in one bullet list. |

## Default advisories

On by default but informational: they report but do not fail `--check`.

| Rule | Fix | Description |
| --- | --- | --- |
| [`duplicate-heading`](duplicate-heading.md) | no | Two headings at the same level under the same parent with the same text. |
| [`unicodeable-subscript`](unicodeable-subscript.md) | yes | Braced super/subscript that has a single-codepoint Unicode form. |
| [`info-string-typo`](info-string-typo.md) | no | Fenced code block info string not in the known-languages allowlist. |

## Opt-in rules

Off by default. Enable with `lint.extend-select = ["name"]` in configuration.

| Rule | Fix | Description |
| --- | --- | --- |
| [`math/render-compat`](math/render-compat.md) | no | Math-renderer compatibility for inline and display math (MathJax v3 / KaTeX). |
| [`list-tightness-flipped`](list-tightness-flipped.md) | no | list tightness from the tree disagrees with tightness from source bytes |
| [`stray-dollar`](stray-dollar.md) | yes | Literal `$` in prose (opt-in for projects that don't use $…$ math). |
| [`latex-command`](latex-command.md) | yes | LaTeX control sequence in prose (opt-in for Unicode-math projects). |
| [`escaped-emphasis`](escaped-emphasis.md) | yes | Literal `\_`, `\*`, or `` \` `` escape in prose (mdformat damage). |
| [`subscript-damage`](subscript-damage.md) | yes | Identifier with `*` where a `_` subscript was expected (formatter damage). |
