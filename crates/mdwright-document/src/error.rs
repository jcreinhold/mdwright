use std::fmt;

/// Markdown source could not be parsed safely.
///
/// `CommonMark` accepts every byte string after mdwright's source
/// canonicalisation, but the parser implementation can still fail on
/// malformed edge cases. `ParseError` keeps that dependency failure at
/// the document boundary instead of exposing parser internals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    input_len: usize,
}

impl ParseError {
    pub(crate) fn parser_panic(input_len: usize) -> Self {
        Self { input_len }
    }

    /// Length in bytes of the canonical parser input.
    #[must_use]
    pub fn input_len(&self) -> usize {
        self.input_len
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Markdown parser failed while reading {} byte(s)", self.input_len)
    }
}

impl std::error::Error for ParseError {}
