//! Paragraphs (CM §4.8).
//!
//! A paragraph is the implicit container for a contiguous block of
//! inline runs separated by blank lines from neighbouring blocks. The
//! type carries no payload: the well-formedness invariant — "inline
//! children, no leading/trailing blank line" — is structural in the
//! surrounding [`crate::tree::Tree`] arena, not per-instance state.
//!
//! The unit struct exists so Phase R prompt 27's dispatcher can
//! exhaust [`crate::cm::block::TypedBlock`] without falling back to
//! the legacy `NodeKind` match for the most common block kind.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Paragraph;

impl Paragraph {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_is_uniquely_inhabited() {
        assert_eq!(Paragraph::new(), Paragraph);
    }
}
