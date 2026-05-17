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
use crate::tree::{NodeId, NodeKind};

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

    /// True iff this paragraph can round-trip through verbatim
    /// emission without losing any normalisation. Used by the
    /// document-root overlay to short-circuit IR-driven emission for
    /// paragraphs whose source bytes already match the canonical form.
    ///
    /// Requirements:
    /// - (a) every inline child is a single-text-segment
    ///   [`InlineRun`](crate::cm::inline::run::InlineRun) (no soft/hard
    ///   breaks, no structural inlines like emphasis/code/links), so
    ///   source-byte emission cannot drop a break the IR would
    ///   otherwise have flattened or rewrapped;
    /// - (b) the wrap policy is [`Wrap::Keep`] — both [`Wrap::No`] and
    ///   [`Wrap::At(_)`] require an IR-driven pass.
    ///
    /// The "no `\r` in source" precondition for any root-verbatim path
    /// lives in `format::block::root_verbatim_safe`; this helper only
    /// answers the paragraph-specific shape question.
    pub(crate) fn is_verbatim_eligible(ctx: &PrettyCtx<'_>, id: NodeId) -> bool {
        if !matches!(ctx.opts.wrap(), Wrap::Keep) {
            return false;
        }
        for child in ctx.tree.children(id) {
            let Some(node) = ctx.tree.node(child) else {
                continue;
            };
            let NodeKind::Run(run) = &node.kind else {
                return false;
            };
            use crate::cm::inline::run::RunPart;
            let mut text_count = 0usize;
            for part in run.parts() {
                match part {
                    RunPart::Text(_) => {
                        text_count = text_count.saturating_add(1);
                        if text_count > 1 {
                            return false;
                        }
                    }
                    RunPart::SoftBreak | RunPart::HardLineBreak | RunPart::HardBreakTag => {
                        return false;
                    }
                }
            }
        }
        true
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
