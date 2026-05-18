# Vendored MyST sample sources

The four `.md` files in this directory are copies from the [`jupyter-book/mystmd`](https://github.com/jupyter-book/mystmd) repository
(MIT license, cloned from `main` on 2026-05-18). They exercise the four MyST/Pandoc constructs that
`prompt 41 — MyST + Pandoc directives` is shipping support for:

| File                | Constructs exercised                                                              | Upstream path                                                                                                         |
| ------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `directives.md`     | `:::{name}` block directives, frontmatter                                         | [`docs/directives.md`](https://github.com/jupyter-book/mystmd/blob/main/docs/directives.md)                           |
| `asides.md`         | `:::{aside}` directive, `{myst:directive}\`name\`` inline roles                   | [`docs/asides.md`](https://github.com/jupyter-book/mystmd/blob/main/docs/asides.md)                                   |
| `blocks.md`         | `+++` block separators (out of scope this session), `%` line comments             | [`docs/blocks.md`](https://github.com/jupyter-book/mystmd/blob/main/docs/blocks.md)                                   |
| `admonitions.md`    | `:::{note}` / `:::{tip}` / etc. directives, options, inline roles, list-table     | [`docs/admonitions.md`](https://github.com/jupyter-book/mystmd/blob/main/docs/admonitions.md)                         |

Total size: ~12.7 KB (under the 50 KB cap in prompt 41 §6).

## How the parity test uses these

`tests/external_corpora.rs` (added in prompt 41) walks every `.md` under `tests/external/` and runs
`mdwright fmt --check` against each. The expectation: idempotence-on-mode under
`FmtOptions::default()` (which has all the directive / inline-role overlays enabled).

If a fixture starts failing after an upstream MyST syntax addition, **do not** silently regenerate.
Either:

1. Land the missing recogniser in mdwright, or
2. Record the divergence in `docs/src/deviations.md` under the MyST-Pandoc parity section
   (analogous to the mdformat-mkdocs parity rows added in prompt 40).

## Updating the vendored copy

To refresh from upstream:

```sh
cd tests/external/jupyter_book_minimal
for f in directives asides blocks admonitions; do
  curl -s "https://raw.githubusercontent.com/jupyter-book/mystmd/main/docs/${f}.md" -o "${f}.md"
done
```

Commit the refresh in a standalone commit so the diff against the prior vendored version is
auditable.

## License

The `mystmd` project is MIT-licensed. See
<https://github.com/jupyter-book/mystmd/blob/main/LICENSE> for the full text.
