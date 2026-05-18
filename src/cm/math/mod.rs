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
//! - [`MathRegion`] values consumed by the format pipeline's overlay
//!   (see `crate::format::block::block_overlaps_math`). The region
//!   carries a [`span::MathSpan`] tag the pretty-printer ([`pretty`])
//!   dispatches on.
//! - [`span::MathError`] values surfaced by the
//!   `math/unbalanced-delim`, `math/unbalanced-env`, and
//!   `math/unbalanced-braces` lint rules.
//!
//! Stack-based tracking enforces `\begin` / `\end` balance with
//! nesting on the same environment name; the four primitive
//! delimiter pairs match greedily on first close.

pub(crate) mod env;
pub(crate) mod pretty;
pub(crate) mod render;
pub(crate) mod scan;
pub(crate) mod span;

use std::ops::Range;

use span::MathSpan;

/// One recognised math region in source order. `range` covers both
/// delimiters and everything between them; the formatter reads this
/// to drive the math overlay in `format::block::pretty_block`. The
/// `span` tag carries the typed classification (inline / display /
/// environment) plus the body byte range that the pretty-printer
/// resolves against source.
#[derive(Clone, Debug)]
pub struct MathRegion {
    pub range: Range<usize>,
    pub(crate) span: MathSpan,
}
