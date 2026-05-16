//! Delimiter classification and recogniser error types.
//!
//! The recogniser ([`super::scan::scan_math_regions`]) returns one
//! [`MathError`] per unmatched opener so the lint rules
//! `math/unbalanced-delim` and `math/unbalanced-env` can surface a
//! useful diagnostic. [`AnyDelim`] is the four-variant tag for which
//! primitive delimiter family is involved; it carries the strings the
//! diagnostic message prints.

use std::ops::Range;

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

/// An unrecoverable shape the recogniser saw. The scanner never
/// panics; it accumulates these and keeps scanning the rest of the
/// document.
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
}
