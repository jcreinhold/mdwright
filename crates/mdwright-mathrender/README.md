# mdwright-mathrender

Math-renderer compatibility profiles and math-body checking for mdwright.

This crate owns the policy "would renderer X with packages P loaded render this
math body?". MathJax v3 and KaTeX are the two profiles available today; future
renderers (MathJax v4, typst, …) go here, not into `mdwright-latex`. The TeX
vocabulary itself lives in `mdwright-latex`; per-renderer compatibility tables
live here.

The public surface is a `Renderer` enum, a `RenderProfile` builder, a single
`check_math_body` function, and a small `RenderIssue` enum. The compatibility
tables are private. See `crates/mdwright-lint/src/stdlib/math_render.rs` for the
lint wiring.
