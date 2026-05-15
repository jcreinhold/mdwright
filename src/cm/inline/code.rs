//! Inline code spans (CM §6.3).
//!
//! [`InlineCodeRun::new`] picks a fence one longer than the longest
//! backtick run inside the body, adds pad spaces when the body starts
//! or ends with a backtick or whitespace, and applies GFM table-cell
//! pipe escaping when [`EscapeScope::in_table_cell`] is set. The
//! result is one byte sequence ready to splice into a `Doc`.

use std::borrow::Cow;

use crate::cm::inline::escape_policy::EscapeScope;

/// A code span whose bytes are the final emission form: fence,
/// optional pad, body (with table-pipe escapes if needed), pad,
/// fence.
#[derive(Clone, Debug)]
pub struct InlineCodeRun<'a> {
    bytes: Cow<'a, str>,
}

impl<'a> InlineCodeRun<'a> {
    #[tracing::instrument(level = "trace", skip(body))]
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn new(body: Cow<'a, str>, scope: EscapeScope) -> Self {
        let longest = longest_backtick_run(body.as_ref());
        let fence_len = longest.saturating_add(1);
        let needs_pad = body.starts_with('`')
            || body.ends_with('`')
            || body.starts_with(' ')
            || body.ends_with(' ');
        let escape_pipe = scope.in_table_cell && body.contains('|');
        let extra = if escape_pipe {
            body.bytes().filter(|&b| b == b'|').count()
        } else {
            0
        };
        let cap = body
            .len()
            .saturating_add(fence_len.saturating_mul(2))
            .saturating_add(usize::from(needs_pad).saturating_mul(2))
            .saturating_add(extra);
        let mut out = String::with_capacity(cap);
        for _ in 0..fence_len {
            out.push('`');
        }
        if needs_pad {
            out.push(' ');
        }
        if escape_pipe {
            let mut last = 0usize;
            for (i, b) in body.bytes().enumerate() {
                if b == b'|' {
                    out.push_str(body.get(last..i).unwrap_or(""));
                    out.push_str("\\|");
                    last = i.saturating_add(1);
                }
            }
            out.push_str(body.get(last..).unwrap_or(""));
        } else {
            out.push_str(body.as_ref());
        }
        if needs_pad {
            out.push(' ');
        }
        for _ in 0..fence_len {
            out.push('`');
        }
        Self {
            bytes: Cow::Owned(out),
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.bytes
    }
}

fn longest_backtick_run(s: &str) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for b in s.bytes() {
        if b == b'`' {
            current = current.saturating_add(1);
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph_scope() -> EscapeScope {
        EscapeScope::default()
    }

    fn table_scope() -> EscapeScope {
        EscapeScope {
            in_table_cell: true,
            ..EscapeScope::default()
        }
    }

    #[test]
    fn plain_body_one_backtick_each_side() {
        let run = InlineCodeRun::new(Cow::Borrowed("foo"), paragraph_scope());
        assert_eq!(run.as_str(), "`foo`");
    }

    #[test]
    fn body_with_backtick_uses_longer_fence() {
        let run = InlineCodeRun::new(Cow::Borrowed("a`b"), paragraph_scope());
        assert_eq!(run.as_str(), "``a`b``");
    }

    #[test]
    fn body_starting_with_backtick_pads() {
        let run = InlineCodeRun::new(Cow::Borrowed("`x"), paragraph_scope());
        assert_eq!(run.as_str(), "`` `x ``");
    }

    #[test]
    fn body_with_long_backtick_run_picks_one_longer() {
        let run = InlineCodeRun::new(Cow::Borrowed("a```b"), paragraph_scope());
        assert_eq!(run.as_str(), "````a```b````");
    }

    #[test]
    fn table_cell_escapes_pipe() {
        let run = InlineCodeRun::new(Cow::Borrowed("a|b"), table_scope());
        assert_eq!(run.as_str(), r"`a\|b`");
    }

    #[test]
    fn paragraph_pipe_is_not_escaped() {
        let run = InlineCodeRun::new(Cow::Borrowed("a|b"), paragraph_scope());
        assert_eq!(run.as_str(), "`a|b`");
    }
}
