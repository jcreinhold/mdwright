//! Math-renderer compatibility profiles and math-body checking.
//!
//! `mdwright-latex` owns the TeX vocabulary and the math-body lexer; this
//! crate owns the *renderer* question: given a configured profile, which
//! commands and environments in a body would actually render? The tables and
//! the package-mask machinery stay private; the public surface is a
//! `Renderer` enum, a `RenderProfile` builder, a single `check_math_body`
//! function, and a small `RenderIssue` enum.
//!
//! Today MathJax v3 and KaTeX are modeled. Adding a new renderer (MathJax v4,
//! typst, …) happens by adding a `Renderer` variant, a constructor on
//! `RenderProfile`, and a new pair of overlay tables in `tables`.

#![forbid(unsafe_code)]
#![allow(
    clippy::doc_markdown,
    reason = "names like MathJax, KaTeX, TeX appear in prose; backticking each one would add noise"
)]

mod check;
mod profile;
mod tables;

pub use check::{RenderIssue, check_math_body};
pub use mdwright_latex::SourceSpan;
pub use profile::{RenderProfile, Renderer};
