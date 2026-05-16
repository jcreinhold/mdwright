//! Fenced and indented code blocks (CM §4.4–§4.5).
//!
//! [`FencedCodeBlock::new`] computes the opening fence length from the
//! body so the chosen fence is strictly longer than the longest body
//! run of its fence character — encoding the d95800d "bucket A" fix
//! from Phase 4 prompt 16 as a constructor invariant. The computation
//! is total, so the constructor is infallible; same for
//! [`IndentedCodeBlock::new`].

use std::borrow::Cow;

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
