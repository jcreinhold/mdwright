# Math rendering

mdwright does not typeset math. It preserves math syntax in your source so a downstream renderer — KaTeX, MathJax,
mkdocs-material's math plugin, jupyter-book — can do the actual rendering. The `--math-render` flag controls the shape
of the math regions in mdwright's formatted output so the downstream renderer recognises them.

For *what* mdwright treats as math, see [Math regions](math-regions.md). This page is about *how those regions are
emitted*.

## The three modes

| Mode | Behaviour |
| --- | --- |
| `none` (default) | Pass math regions through verbatim. |
| `commonmark-katex` | Same emission as `none`, but greppable as an intent signal in build logs and CI output. |
| `dollar` | Rewrite `\[ … \]` to `$$ … $$` and `\( … \)` to `$ … $`. LaTeX environments are not rewritten. |

The default is `none` — mdwright never rewrites math by surprise.

### When to use which

- **`none`** is right for most projects. Both KaTeX (via the `mhchem`/`auto-render` config) and MathJax v3's
    auto-renderer recognise `\[ … \]` and `\( … \)` directly. mkdocs-material's math plugin, jupyter-book, and Pelican
    all work out of the box.
- **`commonmark-katex`** is functionally identical to `none`. Use it in CI when you want grep / log search to confirm
    "yes, this build expects KaTeX downstream" — the mode name leaves a trace.
- **`dollar`** is for pipelines that expect Pandoc-style `$` delimiters. The rewrite is one-directional: `\[ … \]`
    becomes `$$ … $$`, `\( … \)` becomes `$ … $`. Source already in dollar form passes through unchanged. LaTeX
    environments (`\begin{align*} … \end{align*}`) are left alone — there is no dollar form of an environment.

## CLI and config

On the command line, the flag attaches to `mdwright fmt`:

```sh,no-check
mdwright fmt --math-render=dollar path/to/notes.md
```

In `.mdwright.toml`:

```toml,no-check
[fmt.math]
render = "dollar"  # or "none", "commonmark-katex"
```

The CLI flag overrides the config file. Both fall back to `MathRender::None` when unset.

## Inspecting the rendered HTML

`mdwright render` pipes the formatted output through mdwright's HTML renderer to stdout:

```sh,no-check
mdwright render notes.md > notes.html
mdwright render --math-render=dollar notes.md
```

This is a diagnostic surface, not a production renderer. mdwright's HTML emitter does not enable pulldown-cmark's math
extension — math regions land in the HTML as plain text in whatever delimiter form the formatter produced. Feed that
HTML through KaTeX, MathJax, or your static-site generator's math plugin to see the actual typeset output.

## The HTML-equivalence gate

`mdwright fmt` runs every reformat through an HTML-equivalence gate that catches accidental semantic drift. The
straightforward version of that gate compares the source's HTML against the formatted output's HTML; under
`--math-render=dollar`, that comparison would always diverge, because the formatter intentionally rewrites math.

The gate's actual contract is *idempotence-on-mode*: formatting the output a second time with the same options must
produce the same canonical event stream. Round-1-to-round-2 divergence is still a hard failure. See
[`Document::format_validated`](https://docs.rs/mdwright/latest/mdwright/struct.Document.html#method.format_validated)
for the full doc-comment.
