//! Math-region overlay.
//!
//! Math content (`\[ … \]`, `\( … \)`, `$$ … $$`, `$ … $`) is opaque
//! to `CommonMark`: pulldown can't see "math" and will tokenise the
//! bytes inside as plain prose, so `_` becomes emphasis, `[` becomes
//! a link candidate, `*` becomes a delimiter run. Emphasis matching
//! then pairs across what the author intended as TeX subscripts, and
//! `mdwright`'s round-trip drifts.
//!
//! This module gives math content first-class status. The scanner
//! detects regions at the source-byte level and stamps each one with
//! a `range`. Downstream, the tree builder collapses every pulldown
//! event inside a region to a single `NodeKind::Math` leaf, and the
//! inline serializer emits that leaf verbatim — bypassing
//! `escape_text`, flanking analysis, and the wrap pass entirely.
//!
//! Source bytes inside `range` go straight through, so
//! `render_html(source) == render_html(format(source))` holds by
//! construction for math regions.
//!
//! ## Delimiters
//!
//! The four CM-compatible TeX delimiter pairs:
//!
//! | Open  | Close | Display? | Notes                                |
//! |-------|-------|----------|--------------------------------------|
//! | `\[`  | `\]`  | yes      | backslash-escaped `[`/`]`; CM §2.4   |
//! | `\(`  | `\)`  | no       | inline                               |
//! | `$$`  | `$$`  | yes      | matching `$$` pairs                  |
//! | `$`   | `$`   | no       | single `$` pairs (ambiguous w/ $)    |
//!
//! Default config enables `\[ \]` and `\( \)`. The dollar variants
//! are opt-in: `$` is widely used as a literal currency symbol and
//! naïve scanning would misclassify it.
//!
//! ## Backslash-escape parity
//!
//! `\[` is the open iff the `[` is preceded by an *odd*-length run of
//! `\` bytes (CM §2.4: each `\\` is a literal backslash, the trailing
//! `\` is the escape only when the count is odd). `\\[` is therefore
//! a literal `\` followed by a bare `[`, **not** a math open.

use std::ops::Range;

use crate::ir::{CodeBlock, HtmlBlock, InlineCode};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MathDelim {
    /// `\[ … \]`
    BackslashBracket,
    /// `\( … \)`
    BackslashParen,
    /// `$$ … $$`
    DoubleDollar,
    /// `$ … $`
    SingleDollar,
}

impl MathDelim {
    /// `true` for delimiters that semantically introduce display math
    /// (`\[ … \]`, `$$ … $$`). Used by `unbalanced-math-delim` to
    /// distinguish "display" vs "inline" in its diagnostic message.
    pub const fn is_display(self) -> bool {
        matches!(self, Self::BackslashBracket | Self::DoubleDollar)
    }

    pub const fn open(self) -> &'static str {
        match self {
            Self::BackslashBracket => r"\[",
            Self::BackslashParen => r"\(",
            Self::DoubleDollar => "$$",
            Self::SingleDollar => "$",
        }
    }

    pub const fn close(self) -> &'static str {
        match self {
            Self::BackslashBracket => r"\]",
            Self::BackslashParen => r"\)",
            Self::DoubleDollar => "$$",
            Self::SingleDollar => "$",
        }
    }
}

/// One math region in source order. `range` covers both delimiters
/// and everything between them; the formatter reads this to emit
/// math-containing blocks byte-verbatim (see
/// [`crate::format::block`]'s overlap check). Consumers that want
/// just the inner content or the delimiter shape derive them
/// cheaply from the source bytes plus this range.
#[derive(Clone, Debug)]
pub struct MathRegion {
    pub range: Range<usize>,
}

/// An unmatched math open delimiter — `\[` / `\(` / `$$` / `$` with no
/// closing partner before the end of the document or the next
/// exclusion zone (code span, code block, HTML block). Surfaced by
/// [`scan_math`] for the `unbalanced-math-delim` lint rule.
#[derive(Clone, Debug)]
pub struct UnclosedOpen {
    /// Byte range of the open delimiter itself (e.g., the two bytes
    /// of `\[`).
    pub range: Range<usize>,
    pub delim: MathDelim,
}

/// Which math delimiter pairs to recognise. Defaults match the Kan
/// corpus convention (LaTeX-style `\[ \]` and `\( \)`); the dollar
/// variants are opt-in because `$` collides with currency symbols
/// in non-math prose.
#[derive(Copy, Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct MathConfig {
    pub(crate) backslash_bracket: bool,
    pub(crate) backslash_paren: bool,
    pub(crate) double_dollar: bool,
    pub(crate) single_dollar: bool,
}

impl Default for MathConfig {
    fn default() -> Self {
        Self {
            backslash_bracket: true,
            backslash_paren: true,
            double_dollar: false,
            single_dollar: false,
        }
    }
}

/// Scan `source` for math regions, skipping any bytes claimed by
/// existing opaque atoms (inline code spans, fenced/indented code
/// blocks, HTML blocks). Regions never overlap and never extend
/// across an exclusion zone — if an open delimiter has no matching
/// close before the next exclusion, no region is produced.
///
/// Returned regions are in source order, non-overlapping, with
/// well-formed UTF-8 byte boundaries (delimiters are ASCII, so this
/// is automatic).
/// Full scan: returns both closed regions (consumed by the formatter
/// for byte-verbatim emission) and unclosed opens (consumed by the
/// `unbalanced-math-delim` lint rule). Single pass over `source`.
pub(crate) fn scan_math(
    source: &str,
    inline_codes: &[InlineCode<'_>],
    code_blocks: &[CodeBlock<'_>],
    html_blocks: &[HtmlBlock<'_>],
    cfg: MathConfig,
) -> (Vec<MathRegion>, Vec<UnclosedOpen>) {
    let bytes = source.as_bytes();
    let exclusions = build_exclusions(inline_codes, code_blocks, html_blocks);
    let mut regions: Vec<MathRegion> = Vec::new();
    let mut unclosed: Vec<UnclosedOpen> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(end) = excluded_end(&exclusions, i) {
            i = end;
            continue;
        }
        let Some((delim, open_len)) = match_open(bytes, i, cfg) else {
            i = i.saturating_add(1);
            continue;
        };
        let content_start = i.saturating_add(open_len);
        match find_close(bytes, content_start, delim, &exclusions) {
            Some(close_start) => {
                let close_len = delim.close().len();
                let region_end = close_start.saturating_add(close_len);
                regions.push(MathRegion { range: i..region_end });
                i = region_end;
            }
            None => {
                // Unmatched open. Record for the lint rule; advance
                // past the open so the scanner makes progress.
                unclosed.push(UnclosedOpen {
                    range: i..content_start,
                    delim,
                });
                i = content_start;
            }
        }
    }
    (regions, unclosed)
}

/// Sorted, disjoint byte ranges where math regions cannot start or
/// extend through. Built once per `scan_math_regions` call.
fn build_exclusions(
    inline_codes: &[InlineCode<'_>],
    code_blocks: &[CodeBlock<'_>],
    html_blocks: &[HtmlBlock<'_>],
) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::with_capacity(
        inline_codes
            .len()
            .saturating_add(code_blocks.len())
            .saturating_add(html_blocks.len()),
    );
    for c in inline_codes {
        ranges.push(c.raw_range.clone());
    }
    for c in code_blocks {
        ranges.push(c.raw_range.clone());
    }
    for h in html_blocks {
        ranges.push(h.raw_range.clone());
    }
    ranges.sort_by_key(|r| r.start);
    // Merge overlapping spans so a binary search on `start` suffices.
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges {
        if let Some(last) = merged.last_mut()
            && last.end >= r.start
        {
            if r.end > last.end {
                last.end = r.end;
            }
        } else {
            merged.push(r);
        }
    }
    merged
}

/// If position `i` falls inside any exclusion, return that exclusion's
/// end (so the caller can skip past it). Otherwise `None`.
fn excluded_end(exclusions: &[Range<usize>], i: usize) -> Option<usize> {
    let idx = exclusions.partition_point(|r| r.start <= i);
    if let Some(prev_idx) = idx.checked_sub(1)
        && let Some(r) = exclusions.get(prev_idx)
        && i < r.end
    {
        return Some(r.end);
    }
    None
}

/// Match an opening delimiter at position `i`. Returns the matched
/// delimiter and the byte length of the open token.
fn match_open(bytes: &[u8], i: usize, cfg: MathConfig) -> Option<(MathDelim, usize)> {
    let b = bytes.get(i).copied()?;
    match b {
        b'\\' => {
            let next = bytes.get(i.saturating_add(1)).copied()?;
            // CM §2.4: a `\X` escape requires an *odd*-length run of
            // preceding `\`s to terminate at this backslash. We're
            // looking at the start of such a run, so the parity check
            // is on the bytes before `i`.
            if !preceding_backslashes_even(bytes, i) {
                return None;
            }
            match next {
                b'[' if cfg.backslash_bracket => Some((MathDelim::BackslashBracket, 2)),
                b'(' if cfg.backslash_paren => Some((MathDelim::BackslashParen, 2)),
                _ => None,
            }
        }
        b'$' => {
            let two = bytes.get(i.saturating_add(1)).copied();
            if cfg.double_dollar && two == Some(b'$') {
                Some((MathDelim::DoubleDollar, 2))
            } else if cfg.single_dollar {
                Some((MathDelim::SingleDollar, 1))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Count the run of `\` bytes ending immediately before `i` and
/// return true iff the count is even (so `bytes[i]` itself starts a
/// fresh, unescaped sequence).
fn preceding_backslashes_even(bytes: &[u8], i: usize) -> bool {
    let mut j = i;
    let mut count = 0usize;
    while j > 0 {
        let prev = j.saturating_sub(1);
        if bytes.get(prev).copied() == Some(b'\\') {
            count = count.saturating_add(1);
            j = prev;
        } else {
            break;
        }
    }
    count.is_multiple_of(2)
}

/// Search for the matching close delimiter starting at `from`. The
/// close must:
///
/// - lie entirely outside any exclusion zone (a code span / code
///   block / HTML block boundary kills the region);
/// - for `\]` / `\)` closes, be a real CM backslash escape (odd
///   preceding-backslash parity);
/// - for `$$` and `$` closes, simply match byte-wise.
fn find_close(bytes: &[u8], from: usize, delim: MathDelim, exclusions: &[Range<usize>]) -> Option<usize> {
    let mut j = from;
    while j < bytes.len() {
        if let Some(end) = excluded_end(exclusions, j) {
            // Math regions don't cross an exclusion boundary. Bail
            // out: this `\[` is unmatched.
            let _ = end;
            return None;
        }
        match delim {
            MathDelim::BackslashBracket | MathDelim::BackslashParen => {
                if bytes.get(j).copied() == Some(b'\\')
                    && bytes.get(j.saturating_add(1)).copied() == Some(close_target(delim))
                    && preceding_backslashes_even(bytes, j)
                {
                    return Some(j);
                }
            }
            MathDelim::DoubleDollar => {
                if bytes.get(j).copied() == Some(b'$') && bytes.get(j.saturating_add(1)).copied() == Some(b'$') {
                    return Some(j);
                }
            }
            MathDelim::SingleDollar => {
                if bytes.get(j).copied() == Some(b'$') {
                    return Some(j);
                }
            }
        }
        j = j.saturating_add(1);
    }
    None
}

const fn close_target(delim: MathDelim) -> u8 {
    match delim {
        MathDelim::BackslashBracket => b']',
        MathDelim::BackslashParen => b')',
        MathDelim::DoubleDollar | MathDelim::SingleDollar => b'$',
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn empty_excludes() -> (
        Vec<InlineCode<'static>>,
        Vec<CodeBlock<'static>>,
        Vec<HtmlBlock<'static>>,
    ) {
        (Vec::new(), Vec::new(), Vec::new())
    }

    fn scan(source: &str) -> (Vec<MathRegion>, Vec<UnclosedOpen>) {
        let (ic, cb, hb) = empty_excludes();
        scan_math(source, &ic, &cb, &hb, MathConfig::default())
    }

    fn regions(source: &str) -> Vec<MathRegion> {
        scan(source).0
    }

    #[test]
    fn display_math_single_line() {
        let s = r"prefix \[ A \] suffix";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], r"\[ A \]");
    }

    #[test]
    fn display_math_multi_line() {
        let s = "before \\[\n  A \\to B\n\\] after";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        let span = &s[regs[0].range.clone()];
        assert!(span.starts_with(r"\["));
        assert!(span.ends_with(r"\]"));
    }

    #[test]
    fn inline_math_paren() {
        let s = r"x is \( a + b \) units";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], r"\( a + b \)");
    }

    #[test]
    fn two_separate_regions() {
        let s = r"see \[ A \] and \[ B \] both";
        let regs = regions(s);
        assert_eq!(regs.len(), 2);
        assert!(regs[0].range.end <= regs[1].range.start);
    }

    #[test]
    fn unbalanced_open_drops_region_and_emits_diagnostic() {
        let s = r"start \[ no close here";
        let (regs, unclosed) = scan(s);
        assert!(regs.is_empty());
        assert_eq!(unclosed.len(), 1);
        assert_eq!(unclosed[0].delim, MathDelim::BackslashBracket);
        assert!(unclosed[0].delim.is_display());
        assert_eq!(unclosed[0].delim.open(), r"\[");
        assert_eq!(unclosed[0].delim.close(), r"\]");
    }

    #[test]
    fn greedy_first_close() {
        // `\[ a \[ b \] c \]` — the first `\]` closes the region.
        let s = r"\[ a \[ b \] c \]";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], r"\[ a \[ b \]");
    }

    #[test]
    fn double_backslash_open_is_not_math() {
        // `\\[` = literal backslash, then bare `[` — not a math open.
        let s = r"foo \\[ not math \] bar";
        assert!(regions(s).is_empty());
    }

    #[test]
    fn triple_backslash_open_is_math() {
        // `\\\[` = literal backslash + `\[` math open.
        let s = r"foo \\\[ A \] bar";
        assert_eq!(regions(s).len(), 1);
    }

    #[test]
    fn region_inside_code_span_excluded() {
        let s = r"text `\[ x \]` more";
        let ic = vec![InlineCode {
            text: r"\[ x \]",
            byte_offset: 5,
            raw_range: 5..14,
        }];
        let (regs, _) = scan_math(s, &ic, &[], &[], MathConfig::default());
        assert!(regs.is_empty());
    }

    #[test]
    fn region_inside_code_block_excluded() {
        let s = "```\n\\[ x \\]\n```";
        let cb = vec![CodeBlock {
            text: r"\[ x \]",
            byte_offset: 4,
            raw_range: 0..s.len(),
            info: Cow::Borrowed(""),
            fenced: true,
        }];
        let (regs, _) = scan_math(s, &[], &cb, &[], MathConfig::default());
        assert!(regs.is_empty());
    }

    #[test]
    fn dollar_variants_off_by_default() {
        let s = "value is $5 today, plus $$2 tomorrow";
        assert!(regions(s).is_empty());
    }

    #[test]
    fn double_dollar_when_enabled() {
        let s = "see $$ x = 5 $$ above";
        let cfg = MathConfig {
            double_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_math(s, &[], &[], &[], cfg);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], "$$ x = 5 $$");
    }

    #[test]
    fn single_dollar_when_enabled() {
        let s = "x is $a + b$";
        let cfg = MathConfig {
            single_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_math(s, &[], &[], &[], cfg);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], "$a + b$");
    }

    #[test]
    fn region_with_subscripts_and_emphasis_chars() {
        // The bug class the module exists for: math interior full of
        // `_` and `*`. The region's range must cover the whole span
        // so the formatter emits it verbatim.
        let s = r"see \[ \pi_A:\Gamma.A\to \Gamma \] above";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        let span = &s[regs[0].range.clone()];
        assert!(span.contains("_A"));
        assert!(span.contains(r"\Gamma"));
    }

    #[test]
    fn regions_dont_overlap_or_misorder() {
        let s = r"\[ a \] mid \( b \) end \[ c \]";
        let regs = regions(s);
        assert_eq!(regs.len(), 3);
        for w in regs.windows(2) {
            assert!(w[0].range.end <= w[1].range.start);
        }
    }
}
