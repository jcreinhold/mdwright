---
name: info-string-typo
default: true
advisory: true
fix: false
since: 0.2.0
---

# info-string-typo

Fenced code block info string not in the known-languages allowlist.

## What it does

Flags fenced code blocks whose info string (the word after the opening fence — e.g. `rust` in
```` ```rust ````) is not in mdwright's allowlist of known languages and tools.

## Why

Renderers ignore unknown info strings (no syntax highlighting); the most common cause is a
typo like ` ```pyhton` or ` ```jsons`. Catching them in the linter is faster than spotting the
unhighlighted block in a preview. The rule is advisory — projects use long-tail languages
mdwright cannot anticipate, so a flagged unknown info string might be legitimate.

The shipped allowlist covers the languages this project uses (Rust, Python, Lean, …) plus the
usual web stack (HTML, CSS, JS/TS, JSON, YAML, …) and common shell/console variants. Extend the
allowlist via `[lint.info-strings] extra = […]` in your config rather than disabling the rule.

## Example (bad)

````markdown
```pyhton
def f(): pass
```
````

## Example (good)

````markdown
```python
def f(): pass
```
````

## Configuration

- Extend allowlist: `[lint.info-strings] extra = ["mycustomlang", "vendor-dsl"]`.
- Disable inline: `<!-- mdwright: allow info-string-typo -->`.
- Disable in config: `[lint] rules = "default,-info-string-typo"`.
- Severity: advisory.

## References

- [CommonMark §4.5 — Fenced code blocks](https://spec.commonmark.org/0.31.2/#fenced-code-blocks).
- Default allowlist: `DEFAULT_ALLOWLIST` in `src/stdlib/info_string_typo.rs`.
