# mdwright

mdwright is a Markdown linter and round-trip formatter built for documentation that contains real mathematics. It is the
tool that maintains the [Kan](https://github.com/jcreinhold/kan) docs.

It makes three commitments.

**Round-trip safe.** `mdwright fmt` is HTML-equivalent to the input: the rendered DOM after formatting is byte-identical
to the rendered DOM before, modulo whitespace inside paragraphs. Where the formatter cannot guarantee equivalence it
refuses to rewrite; the [deviation table](deviations.md) lists every exception with a reproducer.

**Math-resilient.** Inline math (`\( … \)`), display math (`\[ … \]`), and named LaTeX environments pass through
verbatim. The scanner identifies math regions before any other pass touches the document, so the formatter never reflows
`\frac{a}{b}` into `\\frac{a}{b}` and the linter never flags a backslash inside `\begin{align*} … \end{align*}`. See
[Math regions](concepts/math-regions.md) for the design.

**Fast.** On a 2,107-file corpus mdwright runs about 500× faster than `mdformat --check`. The benchmarks live
under [`benches/`](https://github.com/jcreinhold/mdwright/tree/main/benches); the design rationale is in
[Architecture](extending/architecture.md).

## Who this site is for

- **Users** writing Markdown that contains math, code, or strict formatting requirements. Start with
  [Getting started](getting-started.md).
- **CI operators** wiring mdwright into pre-commit hooks, GitHub Actions, or other automation. See
  [Integration](integration/pre-commit.md).
- **Rule authors** extending mdwright with project-specific lints. See
  [Extending → Lint rules](extending/lint-rules.md).

## What this site is not

This is the manual. The narrative pages (concepts, extending) explain the *why*; the reference pages
([rules](rules/index.md), [CLI](reference/cli.md), [public API](reference/public-api.md),
[diagnostic schema](reference/diagnostic-schema.md)) are the source-of-truth *what*. The
[README](https://github.com/jcreinhold/mdwright) is the 90-second pitch.

## Stability

mdwright is pre-1.0. The release surface — public Rust API, CLI, configuration schema, diagnostic JSON, and lint-rule
trait — is documented descriptively at [Public API](reference/public-api.md), but minor versions may include breaking
changes until 1.0; see [Semver policy](reference/semver.md#pre-10-caveats). Patch releases never break.
