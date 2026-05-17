//! Inline code spans (CM §6.3).
//!
//! [`InlineCodeRun::new`] is the single typed boundary for code-span
//! emission. Its constructor takes the *content* the caller wants
//! pulldown to recover on reparse and produces the source bytes that
//! satisfy that round-trip. The construction-time invariant is:
//!
//! > parsing the emitted bytes back through pulldown yields one
//! > `Event::Code(body)` whose content equals the input `body`.
//!
//! The invariant is enforced in code by a debug-build `debug_assert!`
//! at the end of `new`, so any future change to padding, fence
//! selection, or pipe escaping that breaks round-trip fails loudly
//! in every test that constructs an `InlineCodeRun` — not in a
//! fuzz sweep later.
//!
//! Padding rule (CM §6.1): the parser strips one space from each end
//! of a code span **only** when both ends are spaces **and** the
//! content is not entirely spaces. The constructor mirrors that:
//! pad when an edge is a backtick (fence collision), or when both
//! edges are spaces and the content has at least one non-space byte.
//! Padding eagerly (e.g. for one-sided spaces, or all-space content)
//! would either inflate the output across formats or produce bytes
//! that strip back to something other than `body`.

use crate::cm::inline::escape_policy::EscapeScope;

/// A code span whose bytes are the final emission form: fence,
/// optional pad, body (with table-pipe escapes if needed), pad,
/// fence.
#[derive(Clone, Debug)]
pub struct InlineCodeRun {
    bytes: String,
}

impl InlineCodeRun {
    #[tracing::instrument(level = "trace", skip(body))]
    pub(crate) fn new(body: &str, scope: EscapeScope) -> Self {
        let out = build(body, scope);
        debug_assert!(
            reparses_to(&out, body, scope),
            "InlineCodeRun: emitted bytes do not reparse to body — \
             body={:?} bytes={:?} scope={:?}",
            body,
            out,
            scope,
        );
        Self { bytes: out }
    }

    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        &self.bytes
    }

    /// Emit the canonicalised bytes as a single `Doc::Text`. Text is
    /// atomic by the Wadler/Lindig discipline, so the inline code
    /// span stays on one line without an extra `unbreakable` wrapper.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self) -> crate::format::doc::Doc<'b> {
        use crate::format::doc::text;
        text(self.bytes.clone())
    }
}

/// Emit the bytes for a code span whose reparse yields a single
/// `Event::Code(body)`. Caller-side invariant lives in
/// [`InlineCodeRun::new`].
fn build(body: &str, scope: EscapeScope) -> String {
    let longest = longest_backtick_run(body);
    let fence_len = longest.saturating_add(1);
    // Pad iff (a) an edge byte is a backtick — else the fence
    // collides with the body — or (b) both edges are spaces AND
    // the body contains at least one non-space byte. The second
    // case is CM §6.1: the parser strips one space from each end
    // only when both ends have space AND the content is non-blank.
    // Padding eagerly for one-sided spaces or all-space content
    // would either produce strip-mismatched bytes or inflate the
    // content monotonically across format passes.
    let needs_pad = body.starts_with('`')
        || body.ends_with('`')
        || (body.starts_with(' ') && body.ends_with(' ') && body.bytes().any(|b| b != b' '));
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
        out.push_str(body);
    }
    if needs_pad {
        out.push(' ');
    }
    for _ in 0..fence_len {
        out.push('`');
    }
    out
}

/// Debug-only round-trip check: parse `bytes` *in their target
/// embedding context* (paragraph or table cell) and verify the single
/// emitted `Event::Code` has content equal to `body`. The embedding
/// context matters because the GFM table parser decodes `\|` escapes
/// *before* the inline parser sees the cell content, so a code span
/// containing `\|` reparses correctly inside a table cell but not in
/// a paragraph. Compiled away in release builds.
#[cfg(debug_assertions)]
fn reparses_to(bytes: &str, body: &str, scope: EscapeScope) -> bool {
    use pulldown_cmark::{Event, Options, Parser};

    let (input, opts): (String, Options) = if scope.in_table_cell {
        // Wrap the bytes in a minimal GFM table so pulldown's table
        // parser runs over them and applies the escape rules the
        // formatter relied on. The first row is the header; the
        // second is the delimiter line; the third is the data row
        // that carries our code span.
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        (format!("|h|\n|---|\n|{bytes}|\n"), opts)
    } else {
        (bytes.to_owned(), Options::empty())
    };

    let mut found: Option<String> = None;
    for ev in Parser::new_ext(&input, opts) {
        if let Event::Code(s) = ev {
            if found.is_some() {
                return false;
            }
            found = Some(s.into_string());
        }
    }
    found.as_deref() == Some(body)
}
#[cfg(not(debug_assertions))]
fn reparses_to(_: &str, _: &str, _: EscapeScope) -> bool {
    true
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
        let run = InlineCodeRun::new("foo", paragraph_scope());
        assert_eq!(run.as_str(), "`foo`");
    }

    #[test]
    fn body_with_backtick_uses_longer_fence() {
        let run = InlineCodeRun::new("a`b", paragraph_scope());
        assert_eq!(run.as_str(), "``a`b``");
    }

    #[test]
    fn body_starting_with_backtick_pads() {
        let run = InlineCodeRun::new("`x", paragraph_scope());
        assert_eq!(run.as_str(), "`` `x ``");
    }

    #[test]
    fn body_with_long_backtick_run_picks_one_longer() {
        let run = InlineCodeRun::new("a```b", paragraph_scope());
        assert_eq!(run.as_str(), "````a```b````");
    }

    #[test]
    fn table_cell_escapes_pipe() {
        let run = InlineCodeRun::new("a|b", table_scope());
        assert_eq!(run.as_str(), r"`a\|b`");
    }

    #[test]
    fn paragraph_pipe_is_not_escaped() {
        let run = InlineCodeRun::new("a|b", paragraph_scope());
        assert_eq!(run.as_str(), "`a|b`");
    }

    // ----- regression tests for the padding-inflation bug class -----

    /// All-space body: CM §6.1 does NOT strip (strip rule needs
    /// non-space content). Padding would inflate by 2 per format.
    #[test]
    fn body_all_spaces_does_not_pad() {
        let run = InlineCodeRun::new(" ", paragraph_scope());
        assert_eq!(run.as_str(), "` `");
    }

    #[test]
    fn body_three_spaces_does_not_pad() {
        let run = InlineCodeRun::new("   ", paragraph_scope());
        assert_eq!(run.as_str(), "`   `");
    }

    /// One-sided leading space: strip rule needs both ends, so no
    /// padding is needed; reparse yields `" foo"` directly.
    #[test]
    fn body_leading_space_only_does_not_pad() {
        let run = InlineCodeRun::new(" foo", paragraph_scope());
        assert_eq!(run.as_str(), "` foo`");
    }

    #[test]
    fn body_trailing_space_only_does_not_pad() {
        let run = InlineCodeRun::new("foo ", paragraph_scope());
        assert_eq!(run.as_str(), "`foo `");
    }

    /// Both-sided space with non-space content: CM §6.1 strips one
    /// space each end, so the constructor pads to compensate.
    #[test]
    fn body_both_sided_space_with_content_pads() {
        let run = InlineCodeRun::new(" foo ", paragraph_scope());
        assert_eq!(run.as_str(), "`  foo  `");
    }
}
