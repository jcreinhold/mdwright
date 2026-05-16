//! Fenced and indented code blocks (CM §4.4–§4.5).
//!
//! [`FencedCodeBlock::new`] computes the opening fence length from the
//! body so the chosen fence is strictly longer than the longest body
//! run of its fence character — encoding the d95800d "bucket A" fix
//! from Phase 4 prompt 16 as a constructor invariant. The computation
//! is total, so the constructor is infallible; same for
//! [`IndentedCodeBlock::new`].

use std::borrow::Cow;

use crate::format::doc::{Doc, concat, hard_line, text, unbreakable};
use crate::format::pretty::PrettyCtx;
use crate::tree::NodeId;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum CodeFenceChar {
    Backtick,
    Tilde,
}

impl CodeFenceChar {
    pub(crate) fn as_byte(self) -> u8 {
        match self {
            Self::Backtick => b'`',
            Self::Tilde => b'~',
        }
    }
}

/// A code fence: a string of `length` copies of `char`. `length` is
/// always ≥ 3, and (by [`FencedCodeBlock::new`]) strictly greater than
/// the longest run of `char` in the surrounding block's body.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeFence {
    char: CodeFenceChar,
    length: u8,
}

impl CodeFence {
    pub(crate) fn char(self) -> CodeFenceChar {
        self.char
    }

    pub(crate) fn length(self) -> u8 {
        self.length
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FencedCodeBlock<'a> {
    fence: CodeFence,
    info: Cow<'a, str>,
    body: Cow<'a, str>,
}

impl<'a> FencedCodeBlock<'a> {
    /// Infallible: `pick_fence_length` always returns a value in
    /// 3..=255 because the longest run of a single byte in any body
    /// we accept fits in `u8` (bodies larger than 254 contiguous fence
    /// chars are not valid CM in the first place; we saturate at 255
    /// rather than panic).
    #[tracing::instrument(level = "trace", skip(info, body))]
    pub(crate) fn new(char: CodeFenceChar, info: Cow<'a, str>, body: Cow<'a, str>) -> Self {
        let length = pick_fence_length(char, body.as_ref());
        tracing::trace!(target: "mdwright::cm::block", fence_char = ?char, fence_len = length, "picked fence length");
        Self {
            fence: CodeFence { char, length },
            info,
            body,
        }
    }

    pub(crate) fn fence(&self) -> CodeFence {
        self.fence
    }

    pub(crate) fn info(&self) -> &str {
        self.info.as_ref()
    }

    pub(crate) fn body(&self) -> &str {
        self.body.as_ref()
    }

    /// Emit `FENCE INFO\nBODY\nFENCE\n` honouring source-derived fence
    /// char and source-derived fence length when these are at least as
    /// long as the body-minimum. The whole block is wrapped in
    /// [`unbreakable`] so its embedded newlines never enter a wrap run.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self, ctx: &PrettyCtx<'b>, id: NodeId) -> Doc<'b> {
        let body = self.body.trim_end_matches('\n');
        let source_char = source_fence_char(ctx, id);
        let fence_char = source_char.unwrap_or_else(|| char::from(self.fence.char.as_byte()));
        let body_min = usize::from(self.fence.length);
        let source_len = source_fence_len(ctx, id, fence_char).unwrap_or(0);
        let fence_len = source_len.max(body_min).max(3);
        let fence_string: String = std::iter::repeat_n(fence_char, fence_len).collect();
        let fence_str: &str = fence_string.as_str();
        let info = self.info.as_ref();
        let mut open = String::with_capacity(fence_str.len().saturating_add(info.len()));
        open.push_str(fence_str);
        open.push_str(info);
        if body.is_empty() {
            return concat([
                unbreakable(concat([
                    text(open),
                    hard_line(),
                    text(fence_string.clone()),
                ])),
                hard_line(),
            ]);
        }
        let mut tail =
            String::with_capacity(body.len().saturating_add(fence_str.len()).saturating_add(1));
        tail.push_str(body);
        if !tail.ends_with('\n') {
            tail.push('\n');
        }
        tail.push_str(fence_str);
        concat([
            unbreakable(concat([text(open), hard_line(), text(tail)])),
            hard_line(),
        ])
    }
}

/// First non-whitespace byte of the source for `id`, if it is a fence
/// character. Used to preserve a `~~~` source rather than always
/// emitting backticks.
fn source_fence_char(ctx: &PrettyCtx<'_>, id: NodeId) -> Option<char> {
    let raw = ctx.tree.raw_text(id);
    raw.bytes()
        .find(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        .map(char::from)
        .filter(|c| *c == '`' || *c == '~')
}

/// Length of the opening fence run in the source for this code block,
/// when it matches `fence_char`. Returns `None` for indented blocks or
/// when the source can't be inspected.
fn source_fence_len(ctx: &PrettyCtx<'_>, id: NodeId, fence_char: char) -> Option<usize> {
    let fc = fence_char as u8;
    let raw = ctx.tree.raw_text(id);
    let bytes = raw.as_bytes();
    let start = bytes
        .iter()
        .position(|b| !matches!(*b, b' ' | b'\t' | b'\n' | b'\r'))?;
    if bytes.get(start).copied() != Some(fc) {
        return None;
    }
    let mut i = start;
    while bytes.get(i).copied() == Some(fc) {
        i = i.saturating_add(1);
    }
    Some(i.saturating_sub(start))
}

#[derive(Clone, Debug)]
pub(crate) struct IndentedCodeBlock<'a> {
    body: Cow<'a, str>,
}

impl<'a> IndentedCodeBlock<'a> {
    /// Infallible: any string is a valid indented-code body (CM §4.4
    /// imposes no inner structure).
    #[tracing::instrument(level = "trace", skip(body))]
    pub(crate) fn new(body: Cow<'a, str>) -> Self {
        Self { body }
    }

    pub(crate) fn body(&self) -> &str {
        self.body.as_ref()
    }

    /// Emit an indented code block as a canonical backtick-fenced
    /// block. (At the document root the dispatcher's verbatim overlay
    /// short-circuits this path; the fenced emission applies only when
    /// the block is nested inside a container.)
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self, _ctx: &PrettyCtx<'b>, _id: NodeId) -> Doc<'b> {
        let body = self.body.trim_end_matches('\n');
        let fence_len = pick_fence_length(CodeFenceChar::Backtick, body).max(3) as usize;
        let fence_string: String = std::iter::repeat_n('`', fence_len).collect();
        let fence_str: &str = fence_string.as_str();
        if body.is_empty() {
            return concat([
                unbreakable(concat([
                    text(fence_string.clone()),
                    hard_line(),
                    text(fence_string.clone()),
                ])),
                hard_line(),
            ]);
        }
        let mut tail =
            String::with_capacity(body.len().saturating_add(fence_str.len()).saturating_add(1));
        tail.push_str(body);
        if !tail.ends_with('\n') {
            tail.push('\n');
        }
        tail.push_str(fence_str);
        concat([
            unbreakable(concat([
                text(fence_string.clone()),
                hard_line(),
                text(tail),
            ])),
            hard_line(),
        ])
    }
}

/// `max(3, longest_run_of_char_in(body) + 1)`, saturating at `u8::MAX`.
/// CM §4.5: the opening fence must be strictly longer than every body
/// run of the fence character, otherwise the run closes the block.
fn pick_fence_length(char: CodeFenceChar, body: &str) -> u8 {
    let fc = char.as_byte();
    let mut longest = 0u32;
    let mut current = 0u32;
    for b in body.bytes() {
        if b == fc {
            current = current.saturating_add(1);
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
        }
    }
    let needed = longest.saturating_add(1).max(3);
    u8::try_from(needed).unwrap_or(u8::MAX)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn fb(body: &str) -> FencedCodeBlock<'_> {
        FencedCodeBlock::new(
            CodeFenceChar::Backtick,
            Cow::Borrowed(""),
            Cow::Borrowed(body),
        )
    }

    #[test]
    fn empty_body_uses_three_backticks() {
        assert_eq!(fb("").fence().length(), 3);
    }

    #[test]
    fn body_with_two_backticks_still_uses_three() {
        assert_eq!(fb("a``b").fence().length(), 3);
    }

    #[test]
    fn body_with_three_backticks_grows_to_four() {
        assert_eq!(fb("a```b").fence().length(), 4);
    }

    #[test]
    fn body_with_five_backticks_grows_to_six() {
        assert_eq!(fb("a`````b").fence().length(), 6);
    }

    #[test]
    fn tilde_fence_independent_of_backticks() {
        let block = FencedCodeBlock::new(
            CodeFenceChar::Tilde,
            Cow::Borrowed(""),
            Cow::Borrowed("a```b"),
        );
        assert_eq!(block.fence().length(), 3);
    }

    #[test]
    fn fence_length_is_strict_majority_for_long_run() {
        for n in 3u32..=20 {
            let body: String = std::iter::repeat_n('`', n as usize).collect();
            let expected = u8::try_from(n.saturating_add(1)).expect("small");
            assert_eq!(fb(&body).fence().length(), expected, "n = {n}");
        }
    }

    #[test]
    fn indented_body_preserved() {
        let block = IndentedCodeBlock::new(Cow::Borrowed("    foo\n    bar\n"));
        assert_eq!(block.body(), "    foo\n    bar\n");
    }
}
