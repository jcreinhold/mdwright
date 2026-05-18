//! Math-region grammar.
//!
//! TeX-style math (`\[ … \]`, `\( … \)`, `$$ … $$`, `$ … $`,
//! `\begin{env} … \end{env}`) is opaque to `CommonMark`: pulldown
//! tokenises the bytes inside as plain prose, so `_` becomes
//! emphasis, `[` becomes a link candidate, `*` becomes a delimiter
//! run. Without an overlay the formatter's round-trip drifts.
//!
//! This module is the structural recogniser. [`scan::scan_math_regions`]
//! consumes source plus the IR's inline / block atoms and produces:
//!
//! - [`MathRegion`] values consumed by the format pipeline. The region
//!   carries a [`span::MathSpan`] tag with delimiter and body data.
//! - [`span::MathError`] values surfaced by the
//!   `math/unbalanced-delim`, `math/unbalanced-env`, and
//!   `math/unbalanced-braces` lint rules.
//!
//! Stack-based tracking enforces `\begin` / `\end` balance with
//! nesting on the same environment name; the four primitive
//! delimiter pairs match greedily on first close.

#![forbid(unsafe_code)]

pub mod env;
pub mod normalise;
pub mod render;
pub mod scan;
pub mod span;

use std::ops::Range;

pub use scan::{MathConfig, scan_math_regions};
pub use span::{AnyDelim, DisplayDelim, InlineDelim, MathBody, MathError, MathSpan};

/// One recognised math region in source order.
///
/// `range` covers both delimiters and everything between them. The
/// `span` tag carries the typed classification plus the body byte
/// range resolved against source.
#[derive(Clone, Debug)]
pub struct MathRegion {
    pub range: Range<usize>,
    span: MathSpan,
}

impl MathRegion {
    #[must_use]
    pub fn new(range: Range<usize>, span: MathSpan) -> Self {
        Self { range, span }
    }

    #[must_use]
    pub fn span(&self) -> &MathSpan {
        &self.span
    }
}
