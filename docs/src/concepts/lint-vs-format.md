# Lint vs. format

mdwright has two pipelines and four subcommands. They share one event walk over `pulldown-cmark` but otherwise do not
interact: a lint diagnostic never blocks a format pass, and the formatter never depends on lint state.

## The four subcommands

| Subcommand           | Reads | Writes                  | Exit non-zero on                    |
| -------------------- | ----- | ----------------------- | ----------------------------------- |
| `mdwright check`     | files | nothing                 | any diagnostic                      |
| `mdwright fix`       | files | files (safe fixes only) | any unfixed diagnostic              |
| `mdwright fmt`       | files | files (every input)     | parse error                         |
| `mdwright fmt-check` | files | nothing                 | any input that would be reformatted |

`check` is the audit; `fix` is the audit that can mutate; `fmt` is the unconditional rewrite; `fmt-check` is the
rewrite-or-fail-CI variant.

## Why the pipelines are separate

A linter and a formatter answer different questions.

The **linter** asks: "does this Markdown have problems?" Problems are local — a bare URL, a mismatched
code fence, a duplicate heading id. Diagnostics carry locations and optional fixes. Rules implement the
[`LintRule`](../extending/lint-rules.md) trait and operate on a flat IR (events with byte spans), so adding a rule is
small and self-contained.

The **formatter** asks: "what is the canonical rendering of this Markdown?" Canonicalisation is structural — wrap at 100
columns, dash-marker lists, ATX headings, sorted reference-link definitions. The formatter walks a typed tree IR where
each construct owns its own `pretty()` method, so adding a formatting option means changing one method, not threading
state through a visitor.

Mixing the two would let lint rules read intermediate format state (fragile) or let the formatter short-circuit on lint
diagnostics (an even worse contract). The split is a deliberate boundary.

## When you want both

Most projects run `mdwright check` and `mdwright fmt-check` together in CI. They are independent: a project may format
with mdwright but disable every default-on lint rule, or run a tight lint set without ever invoking the formatter.

```sh
mdwright check . && mdwright fmt-check .
```

For pre-commit hooks, see [Integration → Pre-commit](../integration/pre-commit.md).

## What `--check` means

The `--check` flag on `mdwright check` makes the command fail (exit 1) when any non-advisory diagnostic fires. By
default, `check` prints diagnostics and exits 0 — useful for tooling that wants to consume the output without aborting.

`mdwright fmt-check` has no `--check` flag; it always exits non-zero if any file would be reformatted. This matches
`rustfmt --check`'s contract.

## See also

- [Suppression comments](suppression-comments.md) — silencing a diagnostic without disabling the rule entirely.
- [Configuration](../configuration.md) — separate `[lint]` and `[fmt]` tables.
- [Rules catalogue](../rules/index.md) — every shipping lint rule.
