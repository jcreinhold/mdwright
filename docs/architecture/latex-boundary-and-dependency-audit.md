# LaTeX Boundary And Dependency Audit

mdwright needs MathJax-scale TeX math support, Unicode terminal layout, and bidirectional source translation. That
language machinery is larger and more volatile than Markdown math-span recognition, so it belongs behind a separate
component boundary.

## Boundary Decision

### Design A: Keep TeX Bodies In `mdwright-math`

This keeps the workspace smaller, but it braids two different decisions:

- where Markdown math regions start and end;
- how a TeX-like math body is tokenised, parsed, rendered, and translated.

Those decisions change for different reasons. Markdown span recognition follows CommonMark, GFM, and mdwright's
math-resilience rules. TeX body support follows MathJax's input vocabulary, Unicode coverage, layout, and translation
rules. Keeping both in one crate would make `mdwright-math` the place for delimiter scanning, parser recovery, command
tables, Unicode grids, and translation loss accounting.

### Design B: Create `mdwright-latex`

`mdwright-latex` hides the TeX body language: lexer, parser, command registry, Unicode layout, and source translation.
`mdwright-math` keeps Markdown delimiter and environment recognition and can delegate the body string to
`mdwright-latex` when callers need rendering or translation.

This is the adopted design. The new crate is not a facade: its public API should stay narrower than its implementation.
Callers should receive parsed/rendered/translated results and typed errors, not lexer tokens, parser cursors, AST
variants, or MathJax table internals.

### Design C: Wrap An Existing LaTeX Crate

The current Rust crates are useful references, but they mostly target LaTeX-to-MathML conversion. mdwright needs Unicode
terminal layout and source translation in both directions. Wrapping a MathML-oriented crate would either leak MathML as
an unwanted intermediate interface or force mdwright to reconstruct TeX structure from an output format.

## Dependency Audit

Audit inputs: `cargo info`, crates.io metadata, reachable repository heads, docs.rs pages, and the official
[MathJax TeX input](https://docs.mathjax.org/en/stable/input/tex/index.html) and
[supported-command](https://docs.mathjax.org/en/stable/input/tex/macros/) docs. MathJax is the coverage target because
it documents both TeX input behavior and the supported macro table; it is not treated as a TeX-engine equivalence claim.

| Crate | Version | License | Signal | API fit | Decision |
| --- | --- | --- | --- | --- | --- |
| `logos` | 0.16.1 | MIT OR Apache-2.0 | Mature lexer crate; high crates.io usage; active docs and repository. | Good fit for byte-span tokenisation when the lexer stays policy-free and parser recovery remains separate. | Accept for the lexer spike and later lexer work. |
| `pulldown-latex` | 0.7.1 | MIT | Reachable repository and docs; moderate use. | Pull parser for LaTeX-to-MathML. It does not expose the TeX AST/control needed for Unicode layout and bidirectional source translation. | Reject as a core dependency; keep as a reference. |
| `tex2math` | 1.2.1 | LGPL-3.0-only | Recent crate, but very low crates.io adoption. | LaTeX-to-MathML conversion and CLI/wasm features. License and output-center do not match mdwright's component boundary. | Reject. |
| `latex2mathml` | 0.2.3 | MIT | Older release; moderate total downloads; reachable repository. | Converts equations to MathML. It does not hide the source-translation or Unicode-layout decisions mdwright needs. | Reject as a core dependency; keep as a reference if fixtures are useful. |
| `math-core` | 0.6.1 | MIT | Recent crate with low adoption; Rust 1.91. | Converts LaTeX equations to MathML Core. The crate center is MathML Core, not Unicode layout or source translation. | Reject as a core dependency; revisit only for conformance fixture ideas. |
| `mathml-latex` | 0.0.3 | MPL-2.0 | Early version, low recent usage, reachable repository. | Converts between MathML and LaTeX, but would put MathML at mdwright's internal boundary. | Reject. |

Low-adoption terminal math rendering crates such as `term-maths` and `tui-math` remain rejected. Terminal delivery code
belongs in `crates/mdwright`; TeX body structure belongs in `mdwright-latex`.

## Standing Boundary

- `mdwright-latex` owns TeX math-body lexing, parsing, command vocabulary, Unicode layout, and source translation.
- `mdwright-math` owns Markdown math-span recognition, delimiter policy, and extraction of math body strings.
- `mdwright-lint` consumes vocabulary through narrow lookup APIs after the command table moves.
- `crates/mdwright` owns CLI commands such as `preview` and the future math translation surface.
- Unsupported TeX is a typed error or visible fallback, never a panic.
