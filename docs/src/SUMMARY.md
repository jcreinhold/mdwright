# Summary

[Introduction](introduction.md)

# User guide

- [Installation](installation.md)
- [Getting started](getting-started.md)
- [Configuration](configuration.md)

# Concepts

- [Round-trip safety](concepts/round-trip-safety.md)
- [Math regions](concepts/math-regions.md)
- [Math rendering](concepts/math-rendering.md)
- [Markdown extensions](concepts/extensions.md)
- [MyST + Pandoc directives](concepts/myst-pandoc.md)
- [Lint vs. format](concepts/lint-vs-format.md)
- [Suppression comments](concepts/suppression-comments.md)

# Formatter

- [Formatter policy](format/policy.md)
- [Style knobs](format/style.md)

# Rules

- [Catalogue](rules/index.md)
  - [adjacent-code-no-space](rules/adjacent-code-no-space.md)
  - [bare-url](rules/bare-url.md)
  - [duplicate-heading](rules/duplicate-heading.md)
  - [duplicate-link-label](rules/duplicate-link-label.md)
  - [escaped-emphasis](rules/escaped-emphasis.md)
  - [heading-punctuation](rules/heading-punctuation.md)
  - [inconsistent-list-marker](rules/inconsistent-list-marker.md)
  - [info-string-typo](rules/info-string-typo.md)
  - [latex-command](rules/latex-command.md)
  - [list-tightness-flipped](rules/list-tightness-flipped.md)
  - [orphan-reference-link](rules/orphan-reference-link.md)
  - [stray-dollar](rules/stray-dollar.md)
  - [subscript-damage](rules/subscript-damage.md)
  - [table-pipe-spacing](rules/table-pipe-spacing.md)
  - [trailing-whitespace](rules/trailing-whitespace.md)
  - [unbalanced-backtick](rules/unbalanced-backtick.md)
  - [unicodeable-subscript](rules/unicodeable-subscript.md)
  - [math/unbalanced-braces](rules/math/unbalanced-braces.md)
  - [math/unbalanced-delim](rules/math/unbalanced-delim.md)
  - [math/unbalanced-env](rules/math/unbalanced-env.md)

# Integration

- [Pre-commit](integration/pre-commit.md)
- [GitHub Actions](integration/github-actions.md)
- [Editor integrations](integration/editor-integrations.md)
- [CI recipes](integration/ci-recipes.md)

# Extending

- [Lint rules](extending/lint-rules.md)
- [Plugin loading](extending/plugin-loading.md)
- [Architecture overview](extending/architecture.md)

# Architecture

- [Crate boundaries](architecture/crate-boundaries.md)
- [Parser boundary](architecture/parser-boundary.md)
- [Formatter rewrite boundary](architecture/formatter-rewrite-boundary.md)
- [Pulldown model](architecture/pulldown-model.md)
- [Test matrix](architecture/test-matrix.md)
- [Stability](architecture/stability.md)
- [mdformat parity](architecture/mdformat-parity.md)
- [Parser backend audit](architecture/parser-backend-audit.md)
- [LaTeX boundary and dependency audit](architecture/latex-boundary-and-dependency-audit.md)

# Reference

- [CLI](reference/cli.md)
- [Diagnostic schema](reference/diagnostic-schema.md)
- [Performance](reference/performance.md)
- [Public API surface](reference/public-api.md)
- [Crates.io release](reference/crates-io-release.md)
- [Release evidence](reference/release-evidence.md)
- [Semver policy](reference/semver.md)
- [Deviations from spec](deviations.md)
