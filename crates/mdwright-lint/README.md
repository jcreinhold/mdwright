# mdwright-lint

Lint diagnostics, rule execution, suppression handling, and the standard rule set for
[mdwright](https://github.com/jcreinhold/mdwright).

Rules implement the `LintRule` trait and read from a `Document` produced by
`mdwright-document`. Execution flows through a `RuleSet`, which is also the extension
point the `mdwright` binary uses via `mdwright::run_with_rules` to ship third-party rules
alongside the standard catalogue under `stdlib`. Diagnostics carry severity, snippets,
optional safe fixes, and the docs URL surfaced by `mdwright explain <rule>`.

This crate does not parse Markdown, format files, or load configuration — those belong to
the sibling crates. Adding a new rule means adding a `LintRule` here; adding a new fact
the rule needs to read means going upstream into `mdwright-document` first.

## Status

Pre-1.0. Public items are whatever `lib.rs` re-exports; breaking changes ship without
deprecation warnings.

## Use it

```toml
[dependencies]
mdwright-lint = "0.1"
```

```rust
use mdwright_lint::{LintRule, RuleSet, Diagnostic};
```

## See also

- Project: <https://github.com/jcreinhold/mdwright>
- Rules catalogue: <https://jcreinhold.github.io/mdwright/rules/index.html>
- Plugin extension model:
  <https://jcreinhold.github.io/mdwright/extending/plugin-loading.html>

## License

Licensed under MIT or Apache-2.0, at your option.
