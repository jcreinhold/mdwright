# Round-trip safety

`mdwright fmt` is a *semantic* rewriter, not a string-level one. The contract: the rendered HTML
of the output matches the rendered HTML of the input, modulo whitespace inside a paragraph that
does not change word boundaries. The `gfm_spec_snapshot` test enforces it on every commit. Any
input that fails the gate is either fixed at the root or recorded in the
[deviation table](../deviations.md) with a one-line reason.

## The HTML-equivalence gate

For every document mdwright formats, the gate runs:

1. Render the input to HTML.
2. Format the input, then render the output to HTML.
3. Assert (1) and (2) match, ignoring whitespace-only differences inside text paragraphs.

"Render" here means parse with `pulldown-cmark` and emit HTML from the event stream, so a parse
divergence is caught in the same comparison. If the assertion fails the formatter has changed
semantics; there is no exception path.

## What "semantic" buys you

Some syntactically-equivalent rewrites are not applied. The clearest case: mdwright leaves a
setext heading as-is rather than converting it to ATX when the conversion would change the HTML
id-anchor that external links point at. The cost of round-trip safety is that the formatter
sometimes declines a clean-up it could otherwise perform.

## Reading deviation errors

When the gate trips during development, the test output names the input file, the formatted
output, and the divergent line of HTML. The fix lives in one of three places:

1. **Document recognition** misclassified a span: fix the document facts, not the formatter.
2. **A rewrite producer** proposed a stale or over-broad byte edit: fix the candidate
   owner/range or the verification signature, not the caller.
3. **A new spec case** the existing rules do not handle: extend the recognised facts, then the
   formatter or linter.

## See also

- [Deviations](../deviations.md): every documented exception, with rationale.
- [Architecture](../extending/architecture.md): the two-IR design that makes the gate enforceable.
- [Lint vs. format](lint-vs-format.md): the formatter never relies on linter output.
