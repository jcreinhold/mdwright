//! Paragraphs (CM §4.8).
//!
//! A paragraph carries no per-instance data: its body is its sequence
//! of inline children, and serialisation is the inline body plus a
//! terminating hard newline. The interesting work is the *line-start
//! escape* pass: any text fragment that starts a logical line and
//! happens to open a block construct (ATX heading, list marker,
//! blockquote, fence, thematic break, indented-code prefix) must be
//! backslash-escaped or the round-trip reparses as a different block.

use std::borrow::Cow;

use crate::config::Wrap;
use crate::format::doc::{Doc, concat, hard_line, text};
use crate::format::pretty::PrettyCtx;
use crate::tree::{NodeId, NodeKind};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Paragraph;

impl Paragraph {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new() -> Self {
        Self
    }

    /// Wrap the inline body in line-start escapes and append the
    /// block terminator. `self` is taken by value to keep the typed
    /// dispatcher's method-call form uniform with the other variants.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        let body = crate::format::inline::pretty_inline_children(ctx, id);
        let escaped = escape_paragraph_line_starts(ctx, body);
        concat([escaped, hard_line()])
    }

    /// True iff this paragraph can round-trip through verbatim
    /// emission without losing any normalisation. Used by the
    /// document-root overlay to short-circuit IR-driven emission for
    /// paragraphs whose source bytes already match the canonical form.
    ///
    /// Requirements: (a) every inline child is a single-text-segment
    /// [`InlineRun`](crate::cm::inline::run::InlineRun) (no soft/hard
    /// breaks, no structural inlines like emphasis/code/links), so
    /// source-byte emission cannot drop a break the IR would
    /// otherwise have flattened or rewrapped; (b) the wrap policy is
    /// [`Wrap::Keep`] — both [`Wrap::No`] and [`Wrap::At(_)`] require
    /// an IR-driven pass.
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

// ============================================================
// Line-start escape pass — shared by paragraph and list-item
// rendering.
// ============================================================

pub(crate) fn escape_paragraph_line_starts<'a>(_ctx: &PrettyCtx<'a>, doc: Doc<'a>) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::new();
    flatten(doc, &mut parts);
    coalesce_adjacent_text(&mut parts);
    let mut at_line_start = true;
    let mut out: Vec<Doc<'a>> = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        match part {
            Doc::Text(s) => {
                if at_line_start
                    && !s.is_empty()
                    && let Some(escaped) =
                        escape_for_block_start(s.as_ref(), next_on_same_line(&parts, i))
                {
                    out.push(text(escaped));
                    at_line_start = false;
                    continue;
                }
                if !s.is_empty() {
                    at_line_start = false;
                }
            }
            Doc::HardLine => at_line_start = true,
            Doc::Line | Doc::Concat(_) | Doc::Atomic(_) | Doc::Prefix(_, _) => {
                at_line_start = false;
            }
        }
        out.push(part.clone());
    }
    concat(out)
}

fn next_on_same_line(parts: &[Doc<'_>], i: usize) -> LineContext {
    match parts.get(i.saturating_add(1)) {
        Some(Doc::HardLine) | None => LineContext::EndOfLine,
        Some(_) => LineContext::MoreContent,
    }
}

#[derive(Copy, Clone, Debug)]
enum LineContext {
    MoreContent,
    EndOfLine,
}

fn coalesce_adjacent_text<'a>(parts: &mut Vec<Doc<'a>>) {
    if parts.len() < 2 {
        return;
    }
    let drained: Vec<Doc<'a>> = std::mem::take(parts);
    let mut merged: Vec<Doc<'a>> = Vec::with_capacity(drained.len());
    for part in drained {
        match (merged.last_mut(), part) {
            (Some(Doc::Text(prev)), Doc::Text(next)) => {
                let mut joined = String::with_capacity(prev.len().saturating_add(next.len()));
                joined.push_str(prev.as_ref());
                joined.push_str(next.as_ref());
                *prev = Cow::Owned(joined);
            }
            (_, part) => merged.push(part),
        }
    }
    *parts = merged;
}

fn flatten<'a>(doc: Doc<'a>, out: &mut Vec<Doc<'a>>) {
    match doc {
        Doc::Concat(items) => {
            for item in items.into_vec() {
                flatten(item, out);
            }
        }
        leaf @ (Doc::Text(_) | Doc::Line | Doc::HardLine | Doc::Atomic(_) | Doc::Prefix(_, _)) => {
            out.push(leaf);
        }
    }
}

fn escape_for_block_start(s: &str, next: LineContext) -> Option<String> {
    let bytes = s.as_bytes();
    let first = *bytes.first()?;
    let two: Option<u8> = bytes.get(1).copied();
    let fragment_continues_inline = two.is_none() && matches!(next, LineContext::MoreContent);
    let needs_escape = match first {
        b'#' => true,
        b'>' => true,
        b'-' | b'+' | b'*' => {
            if fragment_continues_inline {
                false
            } else {
                matches!(two, Some(b' ' | b'\t') | None)
                    || (two == Some(first) && bytes.get(2).copied() == Some(first))
            }
        }
        b'=' => two == Some(b'='),
        b'`' | b'~' => two == Some(first) && bytes.get(2).copied() == Some(first),
        b'0'..=b'9' => {
            let mut i = 1usize;
            while i < bytes.len() && bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i = i.saturating_add(1);
            }
            let punct = bytes.get(i).copied();
            let after = bytes.get(i.saturating_add(1)).copied();
            if punct.is_none() && matches!(next, LineContext::MoreContent) {
                false
            } else {
                matches!(punct, Some(b'.' | b')')) && matches!(after, Some(b' ' | b'\t') | None)
            }
        }
        b' ' if bytes.starts_with(b"    ") => true,
        _ => false,
    };
    if !needs_escape {
        return None;
    }
    let mut esc = String::with_capacity(s.len().saturating_add(2));
    if first.is_ascii_digit() {
        let mut i = 0usize;
        while i < bytes.len() && bytes.get(i).is_some_and(u8::is_ascii_digit) {
            esc.push(char::from(*bytes.get(i)?));
            i = i.saturating_add(1);
        }
        esc.push('\\');
        if let Some(b) = bytes.get(i).copied() {
            esc.push(char::from(b));
            i = i.saturating_add(1);
        }
        esc.push_str(s.get(i..)?);
    } else {
        esc.push('\\');
        esc.push_str(s);
    }
    Some(esc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_is_uniquely_inhabited() {
        assert_eq!(Paragraph::new(), Paragraph);
    }
}
