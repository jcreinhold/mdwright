//! Paragraphs (CM §4.8).
//!
//! Unit struct: the IR records the paragraph node. Structural emit
//! preserves the paragraph's source bytes; the wrap pass at
//! [`crate::format::wrap_pass`] is the single owner of paragraph-
//! level line-break decisions.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Paragraph;

impl Paragraph {
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
