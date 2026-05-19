# Lint vs. format

mdwright has two pipelines and four subcommands. They share one event walk over `pulldown-cmark`
but otherwise do not interact: a lint diagnostic never blocks a format pass, and the formatter
never depends on lint state.

## The four subcommands

| Subcommand           | Writes                  | Exit non-zero when                                          |
| -------------------- | ----------------------- | ----------------------------------------------------------- |
| `mdwright check`     | nothing                 | `--check` is set and a non-advisory diagnostic fires        |
| `mdwright fix`       | files (safe fixes only) | `--check` is set and a non-advisory diagnostic still remains |
| `mdwright fmt`       | files (every input)     | parse fails or the safety gate refuses the rewrite (exit 2) |
| `mdwright fmt-check` | nothing                 | any input would be reformatted (exit 1)                     |

`check` is the audit; `fix` is the audit that may mutate; `fmt` is the unconditional rewrite;
`fmt-check` is the rewrite-or-fail-CI variant. By default `check` and `fix` exit 0 even with
diagnostics present—pass `--check` to make them fail CI.

## Why the pipelines are separate

The linter answers a local question: *does this Markdown have problems?* A bare URL, a mismatched
code fence, a duplicate heading id. Diagnostics carry locations and optional fixes. Rules
implement the [`LintRule`](../extending/lint-rules.md) trait and operate on a flat IR (events with
byte spans).

The formatter answers a whole-document question: *which verified byte rewrites should apply?*
Structural emit is identity—default formatting preserves source bytes modulo document-boundary
normalisation. Canonicalisation and wrapping are proposed as rewrite candidates and committed only
after document-level verification.

The two pipelines share a parse but nothing else.

## When you want both

Most projects run both in CI; the two are independent. A project can format with mdwright and
disable every default-on lint, or run a tight lint set without ever invoking the formatter.

```sh
mdwright check . && mdwright fmt-check .
```

For pre-commit hooks, see [Integration → Pre-commit](../integration/pre-commit.md).

## What `--check` means

`--check` on `mdwright check` (or `mdwright fix`) makes the command exit 1 when any non-advisory
diagnostic fires. Without it, `check` prints diagnostics and exits 0—useful for tooling that
wants to consume the output without aborting.

`mdwright fmt-check` has no `--check` flag; it always exits non-zero when any file would be
reformatted, matching `rustfmt --check`'s contract.

## See also

- [Suppression comments](suppression-comments.md)—silencing a diagnostic without disabling the
  rule entirely.
- [Configuration](../configuration.md)—separate `[lint]` and `[fmt]` tables.
- [Rules catalogue](../rules/index.md)—every shipping lint rule.
