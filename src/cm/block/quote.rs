//! Block quotes (CM §5.1).
//!
//! The leading-`>` plus single-space normalisation is encoded by the
//! *type's existence*: a [`BlockQuote`] always means "emit `>` then
//! exactly one space, then the inner line" in `Normalise` mode, and
//! "emit the source bytes verbatim" in `Verbatim` mode. Children live
//! in the surrounding [`crate::tree::Tree`] arena.

use crate::format::doc::{Doc, RenderOptions, concat, hard_line, render, text, unbreakable};
use crate::format::pretty::PrettyCtx;
use crate::format::wrap::wrap_doc;
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

    /// Emit the inner block sequence with every line prefixed by `>`
    /// + space (or bare `>` on blank lines). The inner is rendered to
    /// a string under a wrap budget reduced by the prefix's 2 columns,
    /// then the whole prefixed buffer is emitted as one unbreakable
    /// text block — embedded newlines flow straight through the
    /// renderer.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        let inner = crate::format::block::pretty_block_sequence(ctx, id);
        let wrapped = wrap_doc(inner, ctx.opts.wrap().shrink(2));
        let rendered = render(&wrapped, &RenderOptions);
        let mut prefixed = String::with_capacity(rendered.len().saturating_add(rendered.len() / 32));
        for (i, line) in rendered.split('\n').enumerate() {
            if i > 0 {
                prefixed.push('\n');
            }
            if line.is_empty() {
                prefixed.push('>');
            } else {
                prefixed.push_str("> ");
                prefixed.push_str(line);
            }
        }
        let trimmed = trim_trailing_marker(&prefixed);
        concat([unbreakable(text(trimmed)), hard_line()])
    }
}

fn trim_trailing_marker(s: &str) -> String {
    let mut out = s.to_owned();
    while out.ends_with("\n>") || out.ends_with("\n> ") {
        if let Some(idx) = out.rfind('\n') {
            out.truncate(idx);
        } else {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_quote_is_uniquely_inhabited() {
        assert_eq!(BlockQuote::new(), BlockQuote);
    }
}
