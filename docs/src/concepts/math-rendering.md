# Math rendering

mdwright does not typeset math. It shapes math regions so a downstream renderer, such as KaTeX, MathJax,
mkdocs-material's math plugin, or jupyter-book, can do the typesetting. `--math-render` chooses the shape.

For *what* mdwright treats as math, see [Math regions](math-regions.md). This page is about *how those regions are
emitted*.

## The two modes

| Mode      | Behaviour                                                                    |
| --------- | ---------------------------------------------------------------------------- |
| `none`    | Pass math regions through verbatim. Default.                                 |
| `dollar`  | Rewrite `\[ … \]` to `$$ … $$` and `\( … \)` to `$ … $`. Environments stay.  |

A third value, `commonmark-katex`, is a documentation alias: the behaviour matches `none` exactly, but the name leaves a
greppable signal in CI logs that the build expects KaTeX downstream.

### When to use which

- **`none`** fits most projects. KaTeX (via `auto-render`), MathJax v3's auto-renderer, mkdocs-material's math plugin,
  jupyter-book, and Pelican all recognise `\[ … \]` and `\( … \)` out of the box.
- **`dollar`** fits Pandoc-style pipelines that expect `$` delimiters. The rewrite is one-directional: `\[` becomes
  `$$`, `\(` becomes `$`, source already in dollar form passes through unchanged, and LaTeX environments stay
  environments (there is no dollar form of `\begin{align*}`).

## CLI and config

```sh,no-check
mdwright fmt --math-render=dollar path/to/notes.md
```

```toml,no-check
[fmt.math]
render = "dollar"  # or "none", "commonmark-katex"
```

The CLI flag overrides the config file; both fall back to `MathRender::None`.

## Inspecting the rendered HTML

`mdwright render` pipes the formatted output through mdwright's HTML renderer to stdout:

```sh,no-check
mdwright render notes.md > notes.html
mdwright render --math-render=dollar notes.md
mdwright render --render-profile=cmark-gfm notes.md
```

This is a diagnostic surface, not a production renderer. mdwright's HTML emitter does not enable pulldown-cmark's math
extension: math regions land in the HTML as plain text in whatever delimiter form the formatter produced. Feed that HTML
through KaTeX, MathJax, or your static-site generator's math plugin to see the actual typeset output.

`--render-profile=cmark-gfm` changes HTML spelling only. It is useful when comparing diagnostic HTML with
cmark-gfm-based tools, but it does not change parser semantics or formatter source rewrites.

## The gate under `dollar` mode

The HTML-equivalence gate in [Round-trip safety](round-trip-safety.md) compares pre-format HTML against post-format
HTML. Under `--math-render=dollar` that comparison would always diverge, because the formatter intentionally rewrites
math. The gate's actual contract is *idempotence-on-mode*: formatting the output a second time with the same options
must produce the same canonical event stream. Round-1-to-round-2 divergence is still a hard failure. See
`mdwright_format::format_validated` for the entry point.
