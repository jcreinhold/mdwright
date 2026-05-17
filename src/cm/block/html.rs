//! HTML blocks (CM §4.6).
//!
//! Pulldown-cmark has already classified the surrounding bytes as one
//! of the seven HTML-block opener conditions and accumulated the body
//! verbatim. The typed value carries the body so prompt 27's emitter
//! can write it without re-walking the tree. The bytes are
//! prefix-stripped (no leading container indent / blockquote markers)
//! — the surrounding block emitter re-applies those when needed.

use crate::format::doc::{Doc, concat, hard_line, text};
use crate::format::pretty::PrettyCtx;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlBlock {
    body: String,
}

impl HtmlBlock {
    #[tracing::instrument(level = "trace", skip(body))]
    pub(crate) fn new(body: String) -> Self {
        Self { body }
    }

    #[cfg(test)]
    pub(crate) fn body(&self) -> &str {
        &self.body
    }

    /// Emit the HTML body verbatim, trimming any trailing newlines
    /// before reattaching exactly one as the block terminator. Empty
    /// bodies still emit one hard line so adjacent blocks stay
    /// separated.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self, _ctx: &PrettyCtx<'b>, _id: crate::tree::NodeId) -> Doc<'b> {
        let trimmed = self.body.trim_end_matches('\n');
        if trimmed.is_empty() {
            return hard_line();
        }
        concat([text(trimmed.to_owned()), hard_line()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_round_trips() {
        let b = HtmlBlock::new("<div>x</div>\n".to_owned());
        assert_eq!(b.body(), "<div>x</div>\n");
    }
}
