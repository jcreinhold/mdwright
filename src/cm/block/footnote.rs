//! Footnote definitions (GFM extension; not in CM proper).
//!
//! `[^label]: text` introduces a footnote whose body lives as the
//! definition's children. The typed value carries the raw label as it
//! appeared in source; label normalisation (case folding, whitespace
//! collapse) is the formatter's job at emission time, not the IR's.

use std::borrow::Cow;

use crate::format::doc::{Doc, RenderOptions, concat, hard_line, render, text, unbreakable};
use crate::format::pretty::PrettyCtx;
use crate::format::wrap::wrap_doc;
use crate::tree::NodeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FootnoteDef<'a> {
    label: Cow<'a, str>,
}

impl<'a> FootnoteDef<'a> {
    #[tracing::instrument(level = "trace", skip(label))]
    pub(crate) fn new(label: Cow<'a, str>) -> Self {
        Self { label }
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    /// Emit `[^label]: BODY` with continuation lines indented by 4
    /// spaces. Open multi-line `<!-- ... -->` spans are detected and
    /// have the formatter's own 4-space prefix elided so pulldown's
    /// re-parse sees the same continuation depth.
    ///
    /// TODO(doc-prefix): this construct still uses the legacy
    /// render-to-string-then-prefix pattern because the HTML-comment
    /// continuation rule needs source-aware indent *stripping* on
    /// specific lines — [`crate::format::doc::Doc::Prefix`] applies
    /// indent uniformly and has no per-line content gate. Migrating
    /// this to `prefix_lines` needs the comment-indent compensation
    /// to move into
    /// [`crate::cm::inline::html::InlineHtmlSpan::from_parser`].
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self, ctx: &PrettyCtx<'b>, id: NodeId) -> Doc<'b> {
        let inner = crate::format::block::pretty_block_sequence(ctx, id);
        let first_prefix = self.label.chars().count().saturating_add(5);
        let shrink_n = u32::try_from(first_prefix.max(4)).unwrap_or(u32::MAX);
        let wrapped = wrap_doc(inner, ctx.opts.wrap().shrink(shrink_n));
        let rendered = render(&wrapped, &RenderOptions);
        let trimmed = rendered.trim_end_matches('\n');
        let indent = "    ";
        let mut out = String::new();
        let mut in_comment = false;
        for (i, line) in trimmed.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if i == 0 {
                use std::fmt::Write as _;
                let _ = write!(out, "[^{}]: {}", self.label, line);
            } else if line.is_empty() {
                // blank
            } else if in_comment {
                let stripped = line.strip_prefix(indent).unwrap_or(line);
                out.push_str(indent);
                out.push_str(stripped);
            } else {
                out.push_str(indent);
                out.push_str(line);
            }
            in_comment = update_comment_state(in_comment, line);
        }
        concat([unbreakable(text(out)), hard_line()])
    }
}

fn update_comment_state(start: bool, line: &str) -> bool {
    let mut state = start;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let four = bytes.get(i..i.saturating_add(4));
        let three = bytes.get(i..i.saturating_add(3));
        if !state && four == Some(b"<!--") {
            state = true;
            i = i.saturating_add(4);
        } else if state && three == Some(b"-->") {
            state = false;
            i = i.saturating_add(3);
        } else {
            i = i.saturating_add(1);
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_round_trips() {
        let d = FootnoteDef::new(Cow::Borrowed("foo"));
        assert_eq!(d.label(), "foo");
    }
}
