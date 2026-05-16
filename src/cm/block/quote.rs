//! Block quotes (CM §5.1).
//!
//! The leading-`>` plus single-space normalisation is encoded by the
//! *type's existence*: a [`BlockQuote`] always means "emit `>` then
//! exactly one space, then the inner line" in `Normalise` mode, and
//! "emit the source bytes verbatim" in `Verbatim` mode. Children live
//! in the surrounding [`crate::tree::Tree`] arena.

/// Empty payload by design: every `BlockQuote` has the same emission
/// invariant, so there is no per-instance state to carry. The unit
/// struct is the value-level witness that the surrounding node is a
/// canonicalised block quote.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct BlockQuote;

impl BlockQuote {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_quote_is_uniquely_inhabited() {
        assert_eq!(BlockQuote::new(), BlockQuote);
    }
}
