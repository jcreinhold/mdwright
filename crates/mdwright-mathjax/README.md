# mdwright-mathjax

MathJax compatibility profile and math-body checking for mdwright.

This crate owns the policy "would MathJax render this math body?" — a separable
question from "would mdwright translate it to Unicode?". The TeX vocabulary lives
in `mdwright-latex`; the per-renderer compatibility tables live here. Adding a
new renderer profile (KaTeX, MathJax v4) is meant to happen in this crate, not
by widening `mdwright-latex`'s registry.

The public surface is a profile builder, a single `check_math_body` function,
and a small issue enum. The compatibility tables are private. See `crates/mdwright-lint/src/stdlib/math_mathjax.rs` for the lint wiring.
