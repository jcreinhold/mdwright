# mdwright

mdwright is a Markdown linter and round-trip formatter for technical writing. Math safety is the distinguishing
capability that pushed the design, but the tool is useful on any Markdown.

Three commitments shape the tool.

**Round-trip safe.** `mdwright fmt` renders to the same HTML before and after; every change in the rendered DOM
is treated as a bug. Whitespace inside a paragraph may shift (`a  b` becomes `a b`), but the word boundaries
and the rendered tree do not. Where the formatter cannot prove equivalence it refuses to rewrite; the
[deviation table](deviations.md) lists every exception with a reproducer.

**Math-resilient.** `\( … \)`, `\[ … \]`, and `\begin{NAME} … \end{NAME}` pass through verbatim. The
scanner identifies math regions before any other pass touches the document, so the formatter never reflows
`\frac{a}{b}` into `\\frac{a}{b}` and the linter never flags a backslash inside `\begin{align*}`. See
[Math regions](concepts/math-regions.md) for the design.

**Fast.** Several hundred times faster than `mdformat --check` on a multi-thousand-file corpus. Benches under
[`crates/mdwright/benches/`](https://github.com/jcreinhold/mdwright/tree/main/crates/mdwright/benches); the design
choices that buy this are in [Architecture](extending/architecture.md).

## Who this site is for

- **Users** writing Markdown with math, code, or strict formatting requirements: start with
  [Getting started](getting-started.md).
- **CI operators** wiring mdwright into pre-commit, GitHub Actions, or other automation:
  [Integration](integration/pre-commit.md).
- **Rule authors** extending mdwright with project-specific lints: [Extending → Lint rules](extending/lint-rules.md).

The narrative pages (concepts, extending) explain the *why*; the reference pages ([rules](rules/index.md),
[CLI](reference/cli.md), [public API](reference/public-api.md), [diagnostic schema](reference/diagnostic-schema.md)) are
the source-of-truth *what*.

## Stability

mdwright is pre-1.0. The release surface, including public Rust API, CLI, configuration schema, diagnostic JSON, and
lint-rule trait, is documented descriptively at [Public API](reference/public-api.md); minor versions may include
breaking changes until 1.0, patch releases never do. See [Semver policy](reference/semver.md#pre-10-caveats).
