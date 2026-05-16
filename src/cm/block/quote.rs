//! Block quotes (CM §5.1).
//!
//! The leading-`>` plus single-space normalisation is encoded by the
//! *type's existence*: a [`BlockQuote`] always means "emit `>` then
//! exactly one space, then the inner line" in `Normalise` mode, and
//! "emit the source bytes verbatim" in `Verbatim` mode. Children live
//! in the surrounding [`crate::tree::Tree`] arena.

use crate::format::doc::{Doc, LinePrefix, concat, prefix_lines, text, unbreakable};
use crate::format::pretty::PrettyCtx;
use crate::tree::NodeId;

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

    /// Emit the inner block sequence with every continuation line
    /// prefixed by `>` + space (or bare `>` on blank lines). The
    /// first line's `> ` is pre-pended outside the [`prefix_lines`]
    /// node; the prefix node itself handles every line after the
    /// first hard break.
    ///
    /// The inner sequence already terminates in a `HardLine` (every
    /// block-helper emits one). That trailing `HardLine` plays the
    /// block-terminator role our outer `pretty_block_sequence`
    /// expects, so we deliberately do *not* append another
    /// `hard_line()` here — doing so under a nested blockquote would
    /// leave `pending=AfterHardLine` outside the inner Prefix and
    /// drain the outer prefix's blank form, producing a spurious `>`
    /// row at the end of the quote.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        let inner = crate::format::block::pretty_block_sequence(ctx, id);
        let prefixed = prefix_lines(
            LinePrefix {
                content: "> ".into(),
                blank: ">".into(),
            },
            inner,
        );
        // `unbreakable(text("> "))` keeps the trailing space out of
        // the wrap pass's whitespace-stripping path — otherwise a
        // bare `text("> ")` siblinged into a run loses the space
        // when no following word shares the run, producing `>A…`.
        concat([unbreakable(text("> ")), prefixed])
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
