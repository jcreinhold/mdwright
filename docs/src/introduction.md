# mdwright

mdwright is a Markdown linter and round-trip formatter for any Markdown project.

Four commitments shape the tool.

**Fast.** On a 79-file corpus of math-heavy technical prose, `mdwright fmt-check` runs ≥
50× faster than `mdformat --check`. The multiplier scales with file count and core count;
see [Performance](reference/performance.md) for the measurement, host, and reproducer.
Design choices that buy this are in [Architecture](extending/architecture.md).

**Round-trip safe.** `mdwright fmt` renders to the same HTML before and after; every
change in the rendered DOM is treated as a bug. Whitespace inside a paragraph may shift
(`a  b` becomes `a b`), but word boundaries and the rendered tree do not. Where the
formatter cannot prove equivalence it refuses to rewrite; the
[deviation table](deviations.md) lists every exception with a reproducer.

**Configurable, preserve by default.** Source style choices — emphasis delimiters, list
markers, thematic breaks, link-destination angle brackets — pass through untouched.
Canonicalisation is opt-in one knob at a time in `.mdwright.toml`, or via
`fmt.profile = "mdformat"` for mdformat-compatible spelling where verified rewrites
preserve the parsed document.

**Math-resilient.** `\( … \)`, `\[ … \]`, and `\begin{NAME} … \end{NAME}` pass through
verbatim. The scanner identifies math regions before any other pass touches the document,
so the formatter never reflows `\frac{a}{b}` into `\\frac{a}{b}` and the linter never
flags a backslash inside `\begin{align*}`. See [Math regions](concepts/math-regions.md)
for the design.

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
