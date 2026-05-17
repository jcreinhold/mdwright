# Round-trip safety

`mdwright fmt` is a *semantic* rewriter, not a string-level one. The contract is that the rendered
HTML of the output is byte-identical to the rendered HTML of the input, modulo whitespace inside
paragraphs that does not change word boundaries. The gate is enforced by the `gfm_spec_snapshot`
test on every commit: every input where the gate fails is either fixed at the root or recorded in
the [deviation table](../deviations.md) with a one-line reason.

## The HTML-equivalence gate

For each document mdwright formats, the test pipeline runs:

1. Parse the original input to a `pulldown-cmark` event stream.
2. Render that stream to HTML.
3. Format the input.
4. Parse the formatted output to a `pulldown-cmark` event stream.
5. Render that stream to HTML.
6. Assert (1) and (3) yield the same HTML, ignoring whitespace-only differences inside text
   paragraphs.

If the assertion fails, the formatter has changed semantics. There is no exception path: either
the formatter is fixed or the input lands on the deviation list with a documented reason.

## What "semantic" buys you

Concretely: mdwright will refuse to rewrite a paragraph if doing so would split a sentence across a
list item boundary; it will not collapse two blank lines into one inside a fenced code block; it
will not reflow display math because reflowing math is a category error. The cost is that some
syntactically-equivalent rewrites are *not* applied — a setext heading is left as-is rather than
converted to ATX when the result would change the HTML id-anchor an external link points at.

## Reading deviation errors

When the gate trips during development, the test output names the input file, the formatted output,
and the divergent line of HTML. The typical fix is in one of three places:

1. **The IR builder** (`src/ir.rs`) misclassified a span — fix the recogniser, not the formatter.
2. **The pretty-printer** (`src/format/`) for the misformatted construct — usually a missed
   verbatim copy or a wrong escape policy.
3. **A new spec case** the existing rules do not handle — extend the IR, then the formatter.

Workarounds in the pretty-printer that paper over IR bugs are explicitly out of scope; the
[CLAUDE.md](https://github.com/jcreinhold/mdwright/blob/main/CLAUDE.md#discipline) discipline
document forbids them.

## See also

- [Deviations](../deviations.md) — every documented exception, with rationale.
- [Architecture](../extending/architecture.md) — the two-IR design that makes the gate enforceable.
- [Lint vs. format](lint-vs-format.md) — the formatter never relies on linter output.
