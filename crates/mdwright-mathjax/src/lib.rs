//! `MathJax` compatibility profile and math-body checking.
//!
//! `mdwright-latex` owns the TeX vocabulary and the math-body lexer; this
//! crate owns the *renderer* question: given a configured `MathJax` profile,
//! which commands and environments in a body would actually render? The
//! tables and the package-mask machinery stay private; the public surface is
//! a profile builder, a single check function, and a small diagnostic enum.
//!
//! Today only `MathJax` v3 is modeled. Adding a future v4 or KaTeX profile is
//! meant to happen inside this crate without changing
//! [`check_math_body`]'s signature.

#![forbid(unsafe_code)]
#![allow(
    clippy::doc_markdown,
    reason = "names like MathJax, KaTeX, TeX appear in prose; backticking each one would add noise"
)]

mod check;
mod profile;
mod tables;

pub use check::{MathJaxIssue, check_math_body};
pub use mdwright_latex::SourceSpan;
pub use profile::MathJaxProfile;
