//! Delimiter classification, recogniser error types, and per-region
//! tagged spans.
//!
//! The recogniser ([`super::scan::scan_math_regions`]) produces one
//! [`super::MathRegion`] per recognised math region, each tagged with
//! a [`MathSpan`] that records *which* delimiter family or environment
//! introduced it plus the body byte range. The pretty-printer
//! ([`super::pretty`]) dispatches on the span variant.
//!
//! Unmatched openers and brace-imbalanced bodies become [`MathError`]
//! values so the lint rules `math/unbalanced-delim`,
//! `math/unbalanced-env`, and `math/unbalanced-braces` can surface a
//! useful diagnostic without aborting the scan.

use std::ops::Range;

use super::env::EnvKind;

/// One of the four primitive math delimiter families.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnyDelim {
    /// `\(` / `\)`
    Paren,
    /// `\[` / `\]`
    Bracket,
    /// `$` / `$`
    Dollar,
    /// `$$` / `$$`
    Dollar2,
}

impl AnyDelim {
    pub const fn is_display(self) -> bool {
        matches!(self, Self::Bracket | Self::Dollar2)
    }

    pub const fn open(self) -> &'static str {
        match self {
            Self::Paren => r"\(",
            Self::Bracket => r"\[",
            Self::Dollar => "$",
            Self::Dollar2 => "$$",
        }
    }

    pub const fn close(self) -> &'static str {
        match self {
            Self::Paren => r"\)",
            Self::Bracket => r"\]",
            Self::Dollar => "$",
            Self::Dollar2 => "$$",
        }
    }
}

/// Inline delimiter pair carried on [`MathSpan::Inline`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum InlineDelim {
    /// `\(` / `\)`
    Paren,
    /// `$` / `$`
    Dollar,
}

impl InlineDelim {
    pub(crate) const fn open(self) -> &'static str {
        match self {
            Self::Paren => r"\(",
            Self::Dollar => "$",
        }
    }

    pub(crate) const fn close(self) -> &'static str {
        match self {
            Self::Paren => r"\)",
            Self::Dollar => "$",
        }
    }
}

/// Display delimiter pair carried on [`MathSpan::Display`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum DisplayDelim {
    /// `\[` / `\]`
    Bracket,
    /// `$$` / `$$`
    Dollar2,
}

impl DisplayDelim {
    pub(crate) const fn open(self) -> &'static str {
        match self {
            Self::Bracket => r"\[",
            Self::Dollar2 => "$$",
        }
    }

    pub(crate) const fn close(self) -> &'static str {
        match self {
            Self::Bracket => r"\]",
            Self::Dollar2 => "$$",
        }
    }
}

/// Per-region classification produced by the scanner.
///
/// Each variant carries the byte range of the **body** (between the
/// delimiter or environment-tag tokens). The pretty-printer resolves
/// the range against `PrettyCtx::source`; storing a range instead of a
/// `&'a str` keeps `MathSpan` lifetime-free, matching the existing
/// scanner-IR convention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MathSpan {
    Inline {
        delim: InlineDelim,
        /// Byte range of the body content (excluding the open/close
        /// markers).
        body: Range<usize>,
    },
    Display {
        delim: DisplayDelim,
        body: Range<usize>,
    },
    Environment {
        env: EnvKind,
        body: Range<usize>,
    },
}

impl MathSpan {
    /// Byte range of the body content. Provided so callers do not have
    /// to destructure the enum just to read the body span.
    pub(crate) fn body(&self) -> &Range<usize> {
        match self {
            Self::Inline { body, .. } | Self::Display { body, .. } | Self::Environment { body, .. } => body,
        }
    }
}

/// An unrecoverable shape the recogniser saw. The scanner never
/// panics; it accumulates these and keeps scanning the rest of the
/// document.
//
// The `Unbalanced` prefix is part of the user-facing diagnostic
// vocabulary (it mirrors the rule names `math/unbalanced-delim`,
// `math/unbalanced-env`, `math/unbalanced-braces`), so the
// shared-prefix nudge does not apply here.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub enum MathError {
    /// `\[`, `\(`, `$$`, or `$` with no matching close.
    UnbalancedDelim {
        delim: AnyDelim,
        /// Byte range of the opening delimiter token.
        range: Range<usize>,
    },
    /// `\begin{name}` with no matching `\end{name}` at the same depth.
    UnbalancedEnv {
        name: String,
        /// Byte range covering `\begin{name}` itself.
        range: Range<usize>,
    },
    /// `{` and `}` inside a recognised math body do not balance. The
    /// region still scans (markers are balanced); the pretty-printer
    /// falls back to verbatim emission because we cannot safely
    /// normalise content with broken brace nesting.
    UnbalancedBraces {
        /// Byte offset (absolute, into the source) of the offending
        /// brace — either an unmatched `}` or the start of the body
        /// when the document ends mid-group.
        offset: usize,
        /// Byte range of the math region whose body failed validation.
        region: Range<usize>,
    },
}
