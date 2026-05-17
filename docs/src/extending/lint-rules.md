# Writing a lint rule

A lint rule is a type that implements [`LintRule`][trait]. Rules see the parsed document and emit [`Diagnostic`][diag]
values. mdwright ships ~19 stdlib rules; this page shows how to write a twentieth.

## The trait

```rust
pub trait LintRule: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>);

    fn is_default(&self) -> bool { true }
    fn is_advisory(&self) -> bool { false }
    fn produces_fix(&self) -> bool { false }
    fn explain(&self) -> &'static str { "" }
}
```

- `name` is the kebab-case identifier (`"no-todo-in-prose"`); the dispatcher stamps it onto each emitted diagnostic.
- `description` is the one-line summary shown by `mdwright list-rules`.
- `check` is the actual work — read the [`Document`][doc] and push `Diagnostic` values.
- `is_default` controls whether the rule fires under the `default` rule selector.
- `is_advisory` makes diagnostics informational (they do not fail `--check`).
- `produces_fix` claims that at least one diagnostic carries a [`Fix`][fix].
- `explain` is the long-form markdown shown by `mdwright explain <name>` and rendered into the per-rule doc page.

## Worked example: `no-todo-in-prose`

A rule that flags `TODO` (case-sensitive) inside paragraph text, but not inside code blocks, math regions, or HTML
blocks.

```rust
use mdwright::{Diagnostic, Document, LintRule};

pub struct NoTodoInProse;

impl LintRule for NoTodoInProse {
    fn name(&self) -> &'static str {
        "no-todo-in-prose"
    }

    fn description(&self) -> &'static str {
        "Literal TODO in paragraph text"
    }

    fn explain(&self) -> &'static str {
        "TODOs in user-facing documentation are usually accidents. Track work in an issue \
         tracker; if you want a TODO in the doc, suppress this rule with \
         `<!-- mdwright: allow no-todo-in-prose -->`."
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for slice in doc.text_slices() {
            for (offset, _) in slice.text.match_indices("TODO") {
                if let Some(d) = Diagnostic::at(
                    doc,
                    slice.byte_offset,
                    offset..offset + 4,
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

`doc.text_slices()` yields the text payloads outside math, code, and HTML — exactly the regions where prose lives.
`Diagnostic::at` handles the byte-offset arithmetic and line-index lookup.

## Registering the rule

In your binary or test harness:

```rust
use mdwright::{Document, LintOptions, RuleSet};

let mut rules = RuleSet::stdlib_default();
rules.register(Box::new(NoTodoInProse))?;

let doc = Document::parse(source, &Default::default())?;
let diagnostics = rules.run(&doc, &LintOptions::default());
```

`RuleSet::register` returns `Result<&mut Self, DuplicateRuleName>` so two rules with the same name fail fast.

## Writing the doc page

Each shipped rule has a page under `docs/src/rules/<name>.md`, generated from the rule's metadata by
`cargo xtask doc-rules`. Third-party rules can ship their own pages anywhere; `mdwright explain` will only know about
the rule itself if it is registered into a `RuleSet` your binary uses.

The metadata-driven page contains:

- YAML frontmatter with `name`, `default`, `advisory`, `fix`, `since`.
- The rule's `description` as the H1 tagline.
- The rule's `explain` string, parsed as Markdown.

Format your `explain` string with sections (`## What it does`, `## Why`, `## Example (bad)`, `## Example (good)`) so the
rendered page is consistent with the stdlib rules.

## Loading rules at runtime

In-process plugin loading — a dynamic-library lookup of `LintRule` factories registered by name — is being designed in
[Plugin loading](plugin-loading.md). Until that ships, third-party rules link into a custom mdwright binary.

## See also

- [Architecture](architecture.md) — the IR boundary `LintRule::check` sees.
- [Suppression comments](../concepts/suppression-comments.md) — how rules opt out per-document.
- [Diagnostic schema](../reference/diagnostic-schema.md) — the shape your diagnostics take after the dispatcher stamps
  them.

[diag]: https://docs.rs/mdwright/latest/mdwright/struct.Diagnostic.html
[doc]: https://docs.rs/mdwright/latest/mdwright/struct.Document.html
[fix]: https://docs.rs/mdwright/latest/mdwright/struct.Fix.html
[trait]: https://docs.rs/mdwright/latest/mdwright/trait.LintRule.html
