//! Paragraphs (CM §4.8).
//!
//! A paragraph carries no per-instance data: its body is its sequence
//! of inline children, and serialisation is the inline body plus a
//! terminating hard newline. The line-start escape pass that prevents
//! continuation lines from re-tokenising as a different block lives
//! in [`crate::format::inline::pretty_paragraph_inline`], threaded
//! through the inline-IR walk so escape decisions see the full
//! `RunPart::Text` payload at each line-start position.

use crate::config::Wrap;
use crate::format::doc::{Doc, concat, hard_line};
use crate::format::pretty::PrettyCtx;
use crate::tree::NodeId;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Paragraph;

impl Paragraph {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new() -> Self {
        Self
    }

    /// Build the paragraph body and append the block terminator. The
    /// safety-aware inline walker
    /// ([`crate::format::inline::pretty_paragraph_inline`]) applies
    /// CM line-start escapes to text payloads before any `Doc` is
    /// built, making the "continuation line re-tokenises as a
    /// different block" family of round-trip bugs unrepresentable.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        let body = crate::format::inline::pretty_paragraph_inline(ctx, id);
        concat([body, hard_line()])
    }

    /// True iff `pass1_bytes` — the bytes the IR-driven emit would
    /// produce for this paragraph — match the paragraph's source
    /// bytes (modulo trailing newline). The caller is expected to
    /// render the paragraph through the normal IR path first and
    /// pass the resulting string in.
    ///
    /// This is the output-derived form of the eligibility check: a
    /// paragraph is verbatim-equivalent iff the IR-emit happens to
    /// produce source-equal bytes. Pass-1 emits everything through
    /// the IR; the short-circuit to source-byte emission is only
    /// taken when bytes already agree, which means it can never
    /// silently lose a normalisation.
    ///
    /// Verbatim emission is gated on [`Wrap::Keep`]; [`Wrap::No`] and
    /// [`Wrap::At`] are width policies the verbatim path cannot
    /// honour. The "no `\r` in source" precondition for any root-
    /// verbatim path lives in `format::block::root_verbatim_safe`.
    pub(crate) fn is_verbatim_eligible(ctx: &PrettyCtx<'_>, id: NodeId, pass1_bytes: &str) -> bool {
        if !matches!(ctx.opts.wrap(), Wrap::Keep) {
            return false;
        }
        let source_bytes = ctx.tree.raw_text(ctx.source, id);
        source_bytes.trim_end_matches('\n') == pass1_bytes.trim_end_matches('\n')
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
