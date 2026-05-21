use std::fmt;
use std::ops::Range;

/// Byte span in the original math body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    start: usize,
    end: usize,
}

impl SourceSpan {
    pub(crate) const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// First byte in the span.
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// Byte after the last byte in the span.
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    /// Return this span as a standard byte range.
    #[must_use]
    pub fn as_range(self) -> Range<usize> {
        self.start..self.end
    }
}

/// Category of TeX math-body failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LatexErrorKind {
    /// A byte sequence is not a token in mdwright's TeX math grammar.
    Lexical,
    /// Tokens are well-formed, but the grammar rejects their order.
    Syntax,
    /// The input is valid TeX-like math, but mdwright cannot render or translate it yet.
    Unsupported,
}

/// Typed TeX math-body error with source coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatexError {
    kind: LatexErrorKind,
    span: SourceSpan,
    message: String,
}

impl LatexError {
    pub(crate) fn new(kind: LatexErrorKind, span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }

    /// Error category.
    #[must_use]
    pub const fn kind(&self) -> &LatexErrorKind {
        &self.kind
    }

    /// Source byte span that caused the error.
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        self.span
    }

    /// Human-readable error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for LatexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LatexError {}
