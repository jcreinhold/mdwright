# Extending mdwright with a custom lint rule

This crate shows the supported pattern for adding a third-party lint
rule to mdwright: depend on the library, implement
[`mdwright::LintRule`], and call [`mdwright_cli::run_with_rules`]
from your own `main`. You ship one binary; it has the full mdwright
UX (`check`, `fix`, `fmt`, `lsp`, `--rules`, JSON output, suppression
comments) plus your rule.

## Walkthrough

`src/no_todo.rs` implements `NoTodoInProse` — a rule that flags
literal `TODO` markers in paragraph text, leaving fenced code blocks
and inline code untouched (because [`Document::prose_chunks`] only
yields prose). The rule body is ~25 lines and does no event-stream
work; the typed `Document` accessor handles the math/code/HTML
skipping for you.

`src/main.rs` builds the rule set:

```rust,no_run
fn main() -> std::process::ExitCode {
    let mut rules = mdwright::stdlib::all();
    rules.add(Box::new(NoTodoInProse)).expect("unique name");
    mdwright_cli::run_with_rules(rules)
}
```

That's the whole binary. `run_with_rules` does the rest: clap
parsing, config discovery, output formatting, the LSP server,
everything. Your rule is then selectable like any stdlib rule:

```sh
cargo run -p mdwright-extra-example -- check --rules no-todo-in-prose path/to/docs/
cargo run -p mdwright-extra-example -- list-rules                 # appears in catalogue
cargo run -p mdwright-extra-example -- explain no-todo-in-prose   # prints explain() body
```

Because `NoTodoInProse::is_default()` defaults to `true`, the rule
also fires under `--rules default` (the implicit selector) — exactly
as it would if it were a stdlib rule.

## Publishing your own custom binary

Outside this workspace, the steps are:

1. `cargo new --bin my-mdwright`
1. In `Cargo.toml`, add `mdwright = "0.1"` and `mdwright-cli = "0.1"`
    (whatever version is
    current; the public extension API follows mdwright's
    [semver policy](../../docs/src/reference/semver.md)).
1. Copy `src/no_todo.rs` as a template; replace the rule's logic
    with whatever check you need.
1. Copy `src/main.rs` verbatim.
1. `cargo publish` (or just `cargo build --release` and distribute
    the binary directly).

Downstream users install your binary and run it exactly as they would
the official `mdwright`.

## See also

- `docs/src/extending/lint-rules.md` — the writing-a-rule guide,
    including the `LintRule` trait signature and design notes.
- `docs/src/extending/plugin-loading.md` — why this is the supported
    extension path (and why mdwright does not load plugins at
    runtime).

[`mdwright::LintRule`]: https://docs.rs/mdwright/latest/mdwright/trait.LintRule.html
[`mdwright_cli::run_with_rules`]: https://docs.rs/mdwright-cli/latest/mdwright_cli/fn.run_with_rules.html
[`Document::prose_chunks`]: https://docs.rs/mdwright/latest/mdwright/struct.Document.html#method.prose_chunks
