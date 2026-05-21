//! Math-body language support for mdwright.
//!
//! `mdwright-latex` owns the volatile language machinery for both
//! TeX-like math bodies and parser-backed Unicode mathematical source:
//! lexing, parsing, command vocabulary, Unicode terminal layout, and
//! source translation. Markdown delimiter recognition stays in
//! `mdwright-math`; command-line delivery stays in `mdwright`.
//!
//! The public surface is intentionally narrow. Parser, registry, layout,
//! and translation complexity stays behind result and diagnostic types
//! rather than exposing lexer tokens or AST nodes.

#![forbid(unsafe_code)]

mod error;
mod layout;
#[allow(dead_code, reason = "the private TeX lexer has fixture-tested token APIs")]
mod lexer;
#[allow(dead_code, reason = "the private TeX parser has fixture-tested AST helpers")]
mod parser;
mod registry;
mod translation;
#[allow(dead_code, reason = "the private Unicode lexer has fixture-tested token APIs")]
mod unicode_lexer;
#[allow(dead_code, reason = "the private Unicode parser has fixture-tested AST helpers")]
mod unicode_parser;

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
