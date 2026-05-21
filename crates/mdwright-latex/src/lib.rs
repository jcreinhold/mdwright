//! TeX math-body language support for mdwright.
//!
//! `mdwright-latex` owns the volatile TeX-like language machinery:
//! lexing, parsing, command vocabulary, Unicode terminal layout, and
//! source translation. Markdown delimiter recognition stays in
//! `mdwright-math`; command-line delivery stays in `mdwright`.
//!
//! The public surface is intentionally narrow at this stage. Later
//! parser, registry, layout, and translation work should grow the
//! implementation behind these result and diagnostic types rather than
//! exposing lexer tokens or AST nodes.

#![forbid(unsafe_code)]

mod error;
mod layout;
#[cfg(test)]
mod lexer;
mod parser;
mod registry;
mod translation;

pub use error::{LatexError, LatexErrorKind, SourceSpan};
pub use layout::RenderedLatex;
pub use translation::{Translation, TranslationLoss};
