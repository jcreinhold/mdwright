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
#[allow(dead_code, reason = "parser work consumes this private lexer in the next phase")]
mod lexer;
#[allow(
    dead_code,
    reason = "layout and translation consume this private parser in later phases"
)]
mod parser;
mod registry;
mod translation;
#[allow(
    dead_code,
    reason = "unicode parser and emitter work consumes this private lexer in the next phase"
)]
mod unicode_lexer;

pub use error::{LatexError, LatexErrorKind, SourceSpan};
pub use layout::{RenderedLatex, render_unicode_math};
pub use registry::{
    ArgumentShape, CommandCategory, CommandInfo, SupportStatus, is_known_unsupported_command, latex_symbol,
    lookup_command, unicode_sub, unicode_sub_latex, unicode_sub_str, unicode_super, unicode_super_latex,
    unicode_super_str, unicode_symbol_latex,
};
pub use translation::{
    Translation, TranslationLoss, TranslationStatus, translate_latex_ranges_to_unicode, translate_latex_to_unicode,
    translate_unicode_ranges_to_latex, translate_unicode_to_latex,
};
