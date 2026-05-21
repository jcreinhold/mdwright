# LaTeX boundary

mdwright needs MathJax-scale TeX math support, Unicode terminal layout, and bidirectional source translation. That
language machinery is larger and more volatile than Markdown math-span recognition, so it belongs behind a separate
component boundary.

## The boundary

`mdwright-latex` hides the TeX body language: lexer, parser, command registry, Unicode layout, and source translation.
`mdwright-math` keeps Markdown delimiter and environment recognition and delegates the body string to `mdwright-latex`
when callers need rendering or translation.

`mdwright-latex` is not a facade: its public API stays narrower than its implementation. Callers receive
parsed/rendered/translated results and typed errors, not lexer tokens, parser cursors, AST variants, or MathJax table
internals.

- `mdwright-latex` owns TeX math-body lexing, parsing, command vocabulary, Unicode layout, and source translation.
- `mdwright-math` owns Markdown math-span recognition, delimiter policy, and extraction of math body strings.
- `mdwright-lint` consumes vocabulary through narrow lookup APIs.
- `crates/mdwright` owns CLI commands such as `preview` and the math translation surface.
- Unsupported TeX is a typed error or visible fallback, never a panic.

## Dependency comparison

MathJax is the coverage target because it documents both TeX input behavior and the supported macro table; it is not
treated as a TeX-engine equivalence claim. The comparison axes are licence, signal, API fit at the mdwright boundary,
and outcome.

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

## Rejected boundary shapes

- **Keep TeX bodies in `mdwright-math`.** Braids Markdown span recognition (CommonMark + GFM + math-resilience rules)
  with TeX body support (MathJax input vocabulary, Unicode coverage, layout, translation). The two change for different
  reasons.
- **Wrap an existing LaTeX-to-MathML crate.** The current Rust crates target MathML output. Wrapping one would either
  leak MathML as an unwanted intermediate interface or force mdwright to reconstruct TeX structure from MathML.
