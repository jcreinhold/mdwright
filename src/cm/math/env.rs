//! LaTeX environment classification.
//!
//! Each `\begin{name} … \end{name}` region scanned in
//! [`super::scan::scan_math_regions`] resolves its `name` to one of
//! the variants here. Known environments drive the pretty-printer's
//! per-environment formatting decisions (aligning vs non-aligning,
//! matrix bracketing); unknown names fall through as [`EnvKind::Custom`]
//! and the body is preserved verbatim — we don't pretend to model
//! environments we don't understand.

use std::ops::Range;

/// One of the standard LaTeX / `amsmath` environments the pretty-
/// printer knows how to handle, or an unrecognised name.
///
/// The `Custom` variant carries the byte range of the name inside the
/// source so callers can recover the original spelling without
/// allocating; see [`super::span::MathSpan`] for the consumer side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EnvKind {
    Known(KnownEnv),
    /// Byte range of the environment name in source. The matching
    /// `\end{name}` carries the same name; we store the opening one
    /// because it is reached first.
    Custom(Range<usize>),
}

/// Standard environments recognised by the pretty-printer. Names
/// follow `amsmath` conventions; the starred variants are the
/// unnumbered forms.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum KnownEnv {
    Align,
    AlignStar,
    Aligned,
    Equation,
    EquationStar,
    Gather,
    GatherStar,
    Matrix,
    Pmatrix,
    Bmatrix,
    Vmatrix,
    Smallmatrix,
    Cases,
    Array,
    Split,
    Multline,
    MultlineStar,
}

impl KnownEnv {
    /// Resolve an environment name to a known variant. Returns `None`
    /// for any name outside the standard set; the caller falls back to
    /// [`EnvKind::Custom`].
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "align" => Self::Align,
            "align*" => Self::AlignStar,
            "aligned" => Self::Aligned,
            "equation" => Self::Equation,
            "equation*" => Self::EquationStar,
            "gather" => Self::Gather,
            "gather*" => Self::GatherStar,
            "matrix" => Self::Matrix,
            "pmatrix" => Self::Pmatrix,
            "bmatrix" => Self::Bmatrix,
            "vmatrix" => Self::Vmatrix,
            "smallmatrix" => Self::Smallmatrix,
            "cases" => Self::Cases,
            "array" => Self::Array,
            "split" => Self::Split,
            "multline" => Self::Multline,
            "multline*" => Self::MultlineStar,
            _ => return None,
        })
    }

    /// The canonical name as it appears between `\begin{…}` braces.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Align => "align",
            Self::AlignStar => "align*",
            Self::Aligned => "aligned",
            Self::Equation => "equation",
            Self::EquationStar => "equation*",
            Self::Gather => "gather",
            Self::GatherStar => "gather*",
            Self::Matrix => "matrix",
            Self::Pmatrix => "pmatrix",
            Self::Bmatrix => "bmatrix",
            Self::Vmatrix => "vmatrix",
            Self::Smallmatrix => "smallmatrix",
            Self::Cases => "cases",
            Self::Array => "array",
            Self::Split => "split",
            Self::Multline => "multline",
            Self::MultlineStar => "multline*",
        }
    }

    /// Whether rows in this environment are organised by `&` column
    /// separators that should be aligned by the pretty-printer.
    /// `equation`, `gather`, and `multline` are vertical-list
    /// environments without column alignment.
    pub(crate) const fn is_aligning(self) -> bool {
        matches!(
            self,
            Self::Align
                | Self::AlignStar
                | Self::Aligned
                | Self::Matrix
                | Self::Pmatrix
                | Self::Bmatrix
                | Self::Vmatrix
                | Self::Smallmatrix
                | Self::Cases
                | Self::Array
                | Self::Split
        )
    }
}

impl EnvKind {
    /// `true` when this environment's rows use `&` for column
    /// alignment; `Custom` environments are never aligned (we don't
    /// know their grammar).
    pub(crate) fn is_aligning(&self) -> bool {
        matches!(self, Self::Known(k) if k.is_aligning())
    }

    /// The environment name as it appears in source, borrowed from
    /// `source` for `Custom` and `&'static` for `Known`.
    pub(crate) fn name<'a>(&self, source: &'a str) -> &'a str {
        match self {
            Self::Known(k) => k.name(),
            Self::Custom(range) => source.get(range.clone()).unwrap_or(""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_from_name_round_trips() {
        for k in [
            KnownEnv::Align,
            KnownEnv::AlignStar,
            KnownEnv::Pmatrix,
            KnownEnv::Cases,
            KnownEnv::MultlineStar,
        ] {
            assert_eq!(KnownEnv::from_name(k.name()), Some(k));
        }
    }

    #[test]
    fn unknown_name_returns_none() {
        assert_eq!(KnownEnv::from_name("tikzcd"), None);
        assert_eq!(KnownEnv::from_name("widget"), None);
    }

    #[test]
    fn aligning_split() {
        assert!(KnownEnv::Align.is_aligning());
        assert!(KnownEnv::Pmatrix.is_aligning());
        assert!(KnownEnv::Cases.is_aligning());
        assert!(!KnownEnv::Equation.is_aligning());
        assert!(!KnownEnv::Gather.is_aligning());
        assert!(!KnownEnv::Multline.is_aligning());
    }

    #[test]
    fn custom_is_never_aligning() {
        let env = EnvKind::Custom(0..6);
        assert!(!env.is_aligning());
    }

    #[test]
    fn custom_name_resolves_from_source() {
        let s = "tikzcd";
        let env = EnvKind::Custom(0..6);
        assert_eq!(env.name(s), "tikzcd");
    }
}
