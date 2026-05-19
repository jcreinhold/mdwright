---
name: latex-command
default: false
advisory: false
fix: true
since: 0.1.0
---

# latex-command

LaTeX control sequence in prose (opt-in for Unicode-math projects).

## What it does

Flags TeX-style `\command{…}` invocations in prose (outside math regions)—for example
`\textbf{important}` or `\emph{see below}`.

## Why

LaTeX commands in Markdown prose render literally in most renderers, breaking the visual
result. Authors who write `\textbf{x}` almost always wanted Markdown's `**x**` instead.
Projects targeting Pandoc may legitimately use LaTeX commands; for them, this rule is opt-in
and stays off.

The autofix is conservative: it replaces the command with the equivalent Markdown construct
where one exists (`\textbf{x}` → `**x**`, `\emph{x}` → `*x*`) and skips otherwise.

## Example (bad)

```markdown
This is \textbf{important}.
```

## Example (good)

```markdown
This is **important**.
```

## Configuration

- This rule is off by default. Enable with `[lint] rules = "default,+latex-command"`.
- Disable inline: `<!-- mdwright: allow latex-command -->`.
- Severity: non-advisory. Safe autofix where a Markdown equivalent exists.

## References

- mdwright's command list (`src/stdlib/latex_command.rs`).
