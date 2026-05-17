# Pre-commit

> **Status:** Native `pre-commit` hook ships in a later prompt. The recipe below is the manual equivalent — works today,
> no plugin required.

## Manual hook

Save this as `.git/hooks/pre-commit` and `chmod +x` it:

```sh
#!/usr/bin/env sh
set -e

# Only check staged Markdown files.
files=$(git diff --cached --name-only --diff-filter=ACM | grep -E '\.md$' || true)
[ -z "$files" ] && exit 0

# shellcheck disable=SC2086
mdwright check $files
mdwright fmt-check $files
```

`fmt-check` exits non-zero if any staged file would be reformatted; `check` exits non-zero on any non-advisory
diagnostic.

## With `pre-commit` (the framework)

A first-class `repos:` entry is on the roadmap. Until then, the `local` hook works:

```yaml,no-check
repos:
  - repo: local
    hooks:
      - id: mdwright-check
        name: mdwright check
        entry: mdwright check
        language: system
        types: [markdown]
      - id: mdwright-fmt-check
        name: mdwright fmt-check
        entry: mdwright fmt-check
        language: system
        types: [markdown]
```

## See also

- [GitHub Actions](github-actions.md) — server-side CI gate.
- [Editor integrations](editor-integrations.md) — fix-on-save flow.
