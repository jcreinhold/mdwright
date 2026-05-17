//! Typed emphasis / strong values.
//!
//! [`EmphasisRun`] and [`StrongRun`] capture the source delimiter byte
//! pulldown saw (`*` or `_`) at parse time. The format walker maps that
//! byte to [`EmphasisDelim`] via [`EmphasisRun::resolve`] /
//! [`StrongRun::resolve`] and feeds the result to the safety ladder.
//!
//! Resolution is pure preservation of the parse-time byte: structural
//! emit never consults `FmtOptions`. Style canonicalisation
//! (asterisk-only, underscore-only) is a separate post-pass that
//! rewrites bytes after the structural render.
//!
//! Pulldown only emits an emphasis event when at least one of `*` / `_`
//! is admissible by CM §6.2 in the source position, so the source byte
//! always names a valid delimiter.

/// Resolved delimiter byte for an emphasis or strong run.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum EmphasisDelim {
    Asterisk,
    Underscore,
}

impl EmphasisDelim {
    fn from_byte(byte: u8) -> Self {
        match byte {
            b'_' => Self::Underscore,
            _ => Self::Asterisk,
        }
    }

    /// Swap to the other delimiter. Used by the safety ladder's
    /// collision-avoidance escape strategy when the source delimiter
    /// cannot be emitted safely at this site; the alternative byte is
    /// tried before falling back to body-escaping. Not a style
    /// canonicalisation knob.
    pub(crate) fn flip(self) -> Self {
        match self {
            Self::Asterisk => Self::Underscore,
            Self::Underscore => Self::Asterisk,
        }
    }
}

/// Typed emphasis run. Carries the source delimiter byte pulldown saw
/// at the opening run; body content lives in the tree as children of
/// the enclosing `NodeKind::Emphasis(_)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EmphasisRun {
    source_delim: u8,
}

/// Typed strong run. Same shape as [`EmphasisRun`]; `**` vs `__` is
/// chosen by the same source-byte preservation rule.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StrongRun {
    source_delim: u8,
}

impl EmphasisRun {
    pub(crate) fn from_source(source_delim: u8) -> Self {
        Self { source_delim }
    }

    /// Emit the source delimiter byte as a typed [`EmphasisDelim`].
    /// Never consults `FmtOptions`.
    pub(crate) fn resolve(self) -> EmphasisDelim {
        EmphasisDelim::from_byte(self.source_delim)
    }
}

impl StrongRun {
    pub(crate) fn from_source(source_delim: u8) -> Self {
        Self { source_delim }
    }

    /// Emit the source delimiter byte as a typed [`EmphasisDelim`].
    /// Never consults `FmtOptions`.
    pub(crate) fn resolve(self) -> EmphasisDelim {
        EmphasisDelim::from_byte(self.source_delim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emphasis_preserves_asterisk() {
        assert_eq!(EmphasisRun::from_source(b'*').resolve(), EmphasisDelim::Asterisk);
    }

    #[test]
    fn emphasis_preserves_underscore() {
        assert_eq!(EmphasisRun::from_source(b'_').resolve(), EmphasisDelim::Underscore);
    }

    #[test]
    fn strong_preserves_asterisk() {
        assert_eq!(StrongRun::from_source(b'*').resolve(), EmphasisDelim::Asterisk);
    }

    #[test]
    fn strong_preserves_underscore() {
        assert_eq!(StrongRun::from_source(b'_').resolve(), EmphasisDelim::Underscore);
    }
}
