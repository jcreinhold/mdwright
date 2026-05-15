//! Verbatim emission.
//!
//! Writes a node's source bytes into the `Doc` IR without reparsing,
//! retokenising, or applying any normalisation. The result is a single
//! `Doc::Text(Cow::Borrowed(…))` slice into the source plus a
//! terminating `HardLine`; callers wrap in [`unbreakable`] so the
//! embedded newlines never enter a wrap run.
//!
//! Two callers:
//!
//! 1. The whole-document path when
//!    [`FmtOptions::mode`](crate::config::FmtOptions::mode) is
//!    [`FormatMode::Verbatim`](crate::config::FormatMode::Verbatim) —
//!    every block emits source bytes 1-to-1.
//! 2. Block kinds in [`FormatMode::Normalise`] whose pulldown-cmark
//!    re-tokenisation is the only divergence from the source:
//!    indented code blocks, HTML blocks, and `Text`-only paragraphs.

use crate::format::doc::{Doc, concat, hard_line, text, unbreakable};
use crate::tree::{NodeId, Tree};

/// Emit `id`'s source bytes as a `Doc`: a borrowed `Doc::Text`
/// followed by `HardLine`, wrapped in `unbreakable` so the wrap pass
/// treats the multi-line slice as a single atomic box.
///
/// Allocation-free in the common case: the text payload is
/// `Cow::Borrowed` into `tree.source()`.
#[tracing::instrument(level = "trace", skip(tree))]
pub(crate) fn emit_verbatim<'a>(tree: &Tree<'a>, id: NodeId) -> Doc<'a> {
    let raw = tree.raw_text(id);
    let trimmed = raw.trim_end_matches('\n');
    if trimmed.is_empty() {
        return unbreakable(hard_line());
    }
    unbreakable(concat([text(trimmed), hard_line()]))
}
