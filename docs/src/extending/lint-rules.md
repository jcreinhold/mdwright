# Writing a lint rule

A lint rule is a type that implements [`LintRule`][trait]. Rules see
the parsed document via a curated query surface and emit
[`Diagnostic`][diag] values. mdwright ships nineteen stdlib rules;
this page shows how to add an external twentieth without forking the
binary.

## The trait

```rust
pub trait LintRule: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>);

    fn is_default(&self) -> bool { true }
    fn is_advisory(&self) -> bool { false }
    fn produces_fix(&self) -> bool { false }
    fn explain(&self) -> &str { "" }
}
```

- `name` is the kebab-case identifier (`"no-todo-in-prose"`); the
    dispatcher stamps it onto each emitted diagnostic.
- `description` is the one-line summary shown by `mdwright list-rules`.
- `check` reads the [`Document`][doc] and pushes `Diagnostic` values.
- `is_default` controls whether the rule fires under
    `--rules default` (and the implicit selector when `--rules` is
    omitted).
- `is_advisory` makes diagnostics informational; they do not fail
    `mdwright check --check`.
- `produces_fix` claims that at least one diagnostic carries a
    [`Fix`][fix].
- `explain` is the long-form markdown shown by
    `mdwright explain <name>`.

## Worked example: `no-todo-in-prose`

A rule that flags `TODO` (case-sensitive) inside paragraph text but
not inside code blocks, inline code, math regions, or HTML blocks—
`Document::prose_chunks()` handles every skip for you.

```rust
use mdwright_document::Document;
use mdwright_lint::{Diagnostic, LintRule};

pub struct NoTodoInProse;

impl LintRule for NoTodoInProse {
    fn name(&self) -> &str {
        "no-todo-in-prose"
    }

    fn description(&self) -> &str {
        "Literal TODO in paragraph text"
    }

    fn explain(&self) -> &str {
        "TODOs in user-facing documentation are usually accidents. \
         Track pending work in an issue tracker, or suppress this \
         rule with `<!-- mdwright: allow no-todo-in-prose -->`."
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for slice in doc.prose_chunks() {
            for (offset, _) in slice.text.match_indices("TODO") {
                if let Some(d) = Diagnostic::at(
                    doc,
                    slice.byte_offset,
                    offset..offset + "TODO".len(),
                    "literal `TODO` in prose".to_owned(),
                    None,
                ) {
                    out.push(d);
                }
            }
        }
    }
}
```

`Diagnostic::at` performs the byte-offset arithmetic and line-index
lookup. It returns `Option` because pathological offsets could fall
outside the source; on failure the diagnostic is dropped rather than
the rule panicking.

## Registering the rule

Add it to a [`RuleSet`][ruleset] and lint:

```rust
use mdwright_document::Document;
use mdwright_lint::RuleSet;
# use mdwright_lint::{Diagnostic, LintRule};
# struct NoTodoInProse;
# impl LintRule for NoTodoInProse {
#     fn name(&self) -> &str { "no-todo-in-prose" }
#     fn description(&self) -> &str { "" }
#     fn check(&self, _: &Document, _: &mut Vec<Diagnostic>) {}
# }
let mut rules = RuleSet::stdlib_defaults();
rules.add(Box::new(NoTodoInProse)).expect("unique name");

let doc = Document::parse("My TODO: write the docs.")?;
let diagnostics = rules.check(&doc);
```

[`RuleSet::add`][ruleset-add] returns
`Result<&mut Self, DuplicateRuleName>` so two rules with the same
name fail fast.

## Shipping a custom binary

The CLI crate entry point [`mdwright::run_with_rules`][run] takes
your assembled `RuleSet` and runs the whole CLI on top of it—clap
parsing, config discovery, every output format, the LSP server, the
suppression machinery. Your `main` is ten lines:

```rust,no_run
use mdwright_lint::stdlib;
# struct NoTodoInProse;
# impl mdwright_lint::LintRule for NoTodoInProse {
#     fn name(&self) -> &str { "no-todo-in-prose" }
#     fn description(&self) -> &str { "" }
#     fn check(&self, _: &mdwright_document::Document, _: &mut Vec<mdwright_lint::Diagnostic>) {}
# }

fn main() -> std::process::ExitCode {
    let mut rules = stdlib::all();
    rules.add(Box::new(NoTodoInProse)).expect("unique name");
    mdwright::run_with_rules(rules)
}
```

Pass `stdlib::all()` (not `stdlib::defaults()`) so every opt-in
stdlib rule remains selectable via `--rules`. `run_with_rules`
filters down from this pool based on the user's `--rules` argument
and the active configuration file.

A complete working sample lives at [`examples/extending/`][example]
in the mdwright repository. The same crate has integration tests
that prove the rule fires end-to-end.

## Publishing your custom binary

Your binary is just a crate. Push to crates.io with
`cargo publish` (we recommend a name like `<org>-mdwright` so users
distinguish it from the official binary), or distribute the compiled
artifact directly. Downstream users install your binary and run it
in place of `mdwright`—the command-line interface is identical.

## Caveats

- Config-driven rule reconfiguration applies to stdlib rules only.
    The `[lint.info-strings] extra` option, for example, mutates the
    stdlib `info-string-typo` rule even when a downstream binary has
    registered its own implementation under that name. A `Configurable`
    trait extension is tracked for a later session.
- mdwright does not load lint rules at runtime. See
    [Plugin loading](plugin-loading.md) for the rationale and the
    comparison of dynamic-loading alternatives we considered.

## See also

- [Plugin loading](plugin-loading.md)—why custom binaries are the
    supported path; what we rejected and why.
- [Architecture](architecture.md)—the IR boundary `LintRule::check`
    sees.
- [Suppression comments](../concepts/suppression-comments.md)—how
    rules opt out per-document.
- [Diagnostic schema](../reference/diagnostic-schema.md)—the shape
    your diagnostics take after the dispatcher stamps them.

[diag]: https://docs.rs/mdwright/latest/mdwright/struct.Diagnostic.html
[doc]: https://docs.rs/mdwright/latest/mdwright/struct.Document.html
[example]: https://github.com/jcreinhold/mdwright/tree/main/examples/extending
[fix]: https://docs.rs/mdwright/latest/mdwright/struct.Fix.html
[ruleset]: https://docs.rs/mdwright/latest/mdwright/struct.RuleSet.html
[ruleset-add]: https://docs.rs/mdwright/latest/mdwright/struct.RuleSet.html#method.add
[run]: https://docs.rs/mdwright/latest/mdwright/fn.run_with_rules.html
[trait]: https://docs.rs/mdwright-lint/latest/mdwright_lint/trait.LintRule.html
