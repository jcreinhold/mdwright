//! Structural math recogniser.
//!
//! Walks `source` left-to-right with exclusion zones derived from the
//! IR's inline / block atoms (code spans, code blocks, HTML blocks,
//! inline HTML). Inside an exclusion the scanner skips ahead to the
//! zone's end, so `$` inside `` `cost is $5` `` or `<a title="$x$">`
//! cannot anchor a math region.
//!
//! Three opener families are recognised:
//!
//! - Delimited pairs: `\[ … \]`, `\( … \)`, `$$ … $$`, `$ … $`.
//!   Greedy first-close matches the heuristic scanner's behaviour and
//!   the way `KaTeX` / pandoc resolve these in practice.
//! - LaTeX environments: `\begin{name} … \end{name}`. The recogniser
//!   counts nested `\begin{name}` so an inner environment of the same
//!   name does not close the outer.
//!
//! Unmatched openers become [`MathError`] values without aborting
//! the scan; the rest of the document still produces regions and
//! errors normally.

use std::ops::Range;

use crate::ir::{CodeBlock, HtmlBlock, InlineCode, InlineHtml};

use super::MathRegion;
use super::span::{AnyDelim, MathError};

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
    /// LaTeX `\begin{env}…\end{env}` recognition. Defaults to `true`:
    /// environments outside `\[ \]` are common in mathematical prose
    /// (e.g. raw `\begin{align}` blocks rendered by `KaTeX`) and have
    /// unambiguous closers, unlike `$`.
    pub(crate) environments: bool,
}

impl Default for MathConfig {
    fn default() -> Self {
        Self {
            backslash_bracket: true,
            backslash_paren: true,
            double_dollar: false,
            single_dollar: false,
            environments: true,
        }
    }
}

/// Scan `source` for math regions. Returns regions in source order
/// (non-overlapping) and any unmatched openers as errors.
#[tracing::instrument(level = "debug", skip_all, fields(len = source.len()))]
pub(crate) fn scan_math_regions(
    source: &str,
    inline_codes: &[InlineCode<'_>],
    code_blocks: &[CodeBlock<'_>],
    html_blocks: &[HtmlBlock<'_>],
    inline_html: &[InlineHtml<'_>],
    cfg: MathConfig,
) -> (Vec<MathRegion>, Vec<MathError>) {
    let exclusions = build_exclusions(inline_codes, code_blocks, html_blocks, inline_html);
    let bytes = source.as_bytes();
    let mut regions: Vec<MathRegion> = Vec::new();
    let mut errors: Vec<MathError> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(end) = excluded_end(&exclusions, i) {
            i = end;
            continue;
        }
        // Environments first: `\begin{name}` is structurally
        // unambiguous and would otherwise be passed over.
        if cfg.environments
            && let Some((env_name, after_begin)) = match_begin(source, bytes, i)
        {
            match find_end_env(source, bytes, after_begin, env_name, &exclusions) {
                Some(end_after) => {
                    regions.push(MathRegion {
                        range: i..end_after,
                    });
                    tracing::debug!(env = env_name, range = ?(i..end_after), "env region");
                    i = end_after;
                    continue;
                }
                None => {
                    errors.push(MathError::UnbalancedEnv {
                        name: env_name.to_string(),
                        range: i..after_begin,
                    });
                    i = after_begin;
                    continue;
                }
            }
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
                regions.push(MathRegion {
                    range: i..region_end,
                });
                tracing::debug!(delim = delim.open(), range = ?(i..region_end), "delim region");
                i = region_end;
            }
            None => {
                errors.push(MathError::UnbalancedDelim {
                    delim,
                    range: i..content_start,
                });
                i = content_start;
            }
        }
    }
    (regions, errors)
}

/// Sorted, merged byte ranges where math regions cannot start or
/// extend through.
fn build_exclusions(
    inline_codes: &[InlineCode<'_>],
    code_blocks: &[CodeBlock<'_>],
    html_blocks: &[HtmlBlock<'_>],
    inline_html: &[InlineHtml<'_>],
) -> Vec<Range<usize>> {
    let cap = inline_codes
        .len()
        .saturating_add(code_blocks.len())
        .saturating_add(html_blocks.len())
        .saturating_add(inline_html.len());
    let mut ranges: Vec<Range<usize>> = Vec::with_capacity(cap);
    for c in inline_codes {
        ranges.push(c.raw_range.clone());
    }
    for c in code_blocks {
        ranges.push(c.raw_range.clone());
    }
    for h in html_blocks {
        ranges.push(h.raw_range.clone());
    }
    for h in inline_html {
        ranges.push(h.raw_range.clone());
    }
    ranges.sort_by_key(|r| r.start);
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

/// Match `\begin{name}` at `i`. Returns `(name, position after the
/// closing `}`)`. The `\` must not be itself escaped (even-count of
/// preceding backslashes).
fn match_begin<'a>(source: &'a str, bytes: &[u8], i: usize) -> Option<(&'a str, usize)> {
    let after = match_kw(bytes, i, b"begin")?;
    parse_env_name(source, after)
}

/// Match `\end{name}` at `j`. Returns `(name, position after the
/// closing `}`)`.
fn match_end<'a>(source: &'a str, bytes: &[u8], j: usize) -> Option<(&'a str, usize)> {
    let after = match_kw(bytes, j, b"end")?;
    parse_env_name(source, after)
}

/// Common prefix check for `\begin` / `\end`. Returns the position
/// just after the keyword on success.
fn match_kw(bytes: &[u8], i: usize, keyword: &[u8]) -> Option<usize> {
    if bytes.get(i).copied() != Some(b'\\') {
        return None;
    }
    if !preceding_backslashes_even(bytes, i) {
        return None;
    }
    let kw_start = i.saturating_add(1);
    let kw_end = kw_start.saturating_add(keyword.len());
    if bytes.get(kw_start..kw_end) != Some(keyword) {
        return None;
    }
    Some(kw_end)
}

/// Parse `{name}` starting at `after`, where `name` is `[A-Za-z]+\*?`
/// (LaTeX environment name convention). Returns `(name, position
/// after the closing `}`)`.
fn parse_env_name(source: &str, after: usize) -> Option<(&str, usize)> {
    let bytes = source.as_bytes();
    if bytes.get(after).copied() != Some(b'{') {
        return None;
    }
    let name_start = after.saturating_add(1);
    let mut j = name_start;
    while let Some(b) = bytes.get(j).copied() {
        if b.is_ascii_alphabetic() {
            j = j.saturating_add(1);
        } else {
            break;
        }
    }
    // Optional trailing `*` for the unnumbered variants.
    if bytes.get(j).copied() == Some(b'*') {
        j = j.saturating_add(1);
    }
    if j == name_start {
        return None;
    }
    if bytes.get(j).copied() != Some(b'}') {
        return None;
    }
    let name = source.get(name_start..j)?;
    Some((name, j.saturating_add(1)))
}

/// Find the matching `\end{name}` for an open environment. Returns
/// the byte index just after the closing `}` of `\end{name}`. Counts
/// nested `\begin{name}` so inner environments of the same name do
/// not close the outer.
fn find_end_env(
    source: &str,
    bytes: &[u8],
    from: usize,
    name: &str,
    exclusions: &[Range<usize>],
) -> Option<usize> {
    let mut depth: u32 = 1;
    let mut j = from;
    while j < bytes.len() {
        if let Some(end) = excluded_end(exclusions, j) {
            j = end;
            continue;
        }
        if let Some((found_name, after)) = match_end(source, bytes, j) {
            if found_name == name {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(after);
                }
            }
            j = after;
            continue;
        }
        if let Some((found_name, after)) = match_begin(source, bytes, j) {
            if found_name == name {
                depth = depth.saturating_add(1);
            }
            j = after;
            continue;
        }
        j = j.saturating_add(1);
    }
    None
}

/// Match a primitive delimiter opener at position `i`. Returns the
/// matched delimiter and the byte length of the open token.
fn match_open(bytes: &[u8], i: usize, cfg: MathConfig) -> Option<(AnyDelim, usize)> {
    let b = bytes.get(i).copied()?;
    match b {
        b'\\' => {
            if !preceding_backslashes_even(bytes, i) {
                return None;
            }
            let next = bytes.get(i.saturating_add(1)).copied()?;
            match next {
                b'[' if cfg.backslash_bracket => Some((AnyDelim::Bracket, 2)),
                b'(' if cfg.backslash_paren => Some((AnyDelim::Paren, 2)),
                _ => None,
            }
        }
        b'$' => {
            let two = bytes.get(i.saturating_add(1)).copied();
            if cfg.double_dollar && two == Some(b'$') {
                Some((AnyDelim::Dollar2, 2))
            } else if cfg.single_dollar {
                Some((AnyDelim::Dollar, 1))
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

/// Search for the matching close delimiter starting at `from`.
fn find_close(
    bytes: &[u8],
    from: usize,
    delim: AnyDelim,
    exclusions: &[Range<usize>],
) -> Option<usize> {
    let mut j = from;
    while j < bytes.len() {
        if excluded_end(exclusions, j).is_some() {
            // Math regions don't cross an exclusion boundary.
            return None;
        }
        match delim {
            AnyDelim::Bracket | AnyDelim::Paren => {
                if bytes.get(j).copied() == Some(b'\\')
                    && bytes.get(j.saturating_add(1)).copied() == Some(close_target_byte(delim))
                    && preceding_backslashes_even(bytes, j)
                {
                    return Some(j);
                }
            }
            AnyDelim::Dollar2 => {
                if bytes.get(j).copied() == Some(b'$')
                    && bytes.get(j.saturating_add(1)).copied() == Some(b'$')
                {
                    return Some(j);
                }
            }
            AnyDelim::Dollar => {
                if bytes.get(j).copied() == Some(b'$') {
                    return Some(j);
                }
            }
        }
        j = j.saturating_add(1);
    }
    None
}

const fn close_target_byte(delim: AnyDelim) -> u8 {
    match delim {
        AnyDelim::Bracket => b']',
        AnyDelim::Paren => b')',
        AnyDelim::Dollar2 | AnyDelim::Dollar => b'$',
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn scan(source: &str) -> (Vec<MathRegion>, Vec<MathError>) {
        scan_math_regions(source, &[], &[], &[], &[], MathConfig::default())
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
    fn unbalanced_open_drops_region_and_emits_error() {
        let s = r"start \[ no close here";
        let (regs, errs) = scan(s);
        assert!(regs.is_empty());
        assert_eq!(errs.len(), 1);
        match &errs[0] {
            MathError::UnbalancedDelim { delim, .. } => {
                assert!(delim.is_display());
                assert_eq!(delim.open(), r"\[");
                assert_eq!(delim.close(), r"\]");
            }
            MathError::UnbalancedEnv { .. } => panic!("expected delim error"),
        }
    }

    #[test]
    fn greedy_first_close() {
        let s = r"\[ a \[ b \] c \]";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], r"\[ a \[ b \]");
    }

    #[test]
    fn double_backslash_open_is_not_math() {
        let s = r"foo \\[ not math \] bar";
        assert!(regions(s).is_empty());
    }

    #[test]
    fn triple_backslash_open_is_math() {
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
        let (regs, _) = scan_math_regions(s, &ic, &[], &[], &[], MathConfig::default());
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
        let (regs, _) = scan_math_regions(s, &[], &cb, &[], &[], MathConfig::default());
        assert!(regs.is_empty());
    }

    #[test]
    fn region_inside_inline_html_excluded() {
        // Math-like bytes appearing inside an inline HTML tag (e.g.
        // attribute values) must not anchor a region. The heuristic
        // scanner missed this class because it had no inline-HTML
        // exclusion list.
        let s = r#"see <a href="/x?val=$foo">x</a> after"#;
        let ih = vec![InlineHtml {
            text: r#"<a href="/x?val=$foo">"#,
            byte_offset: 4,
            raw_range: 4..26,
        }];
        let cfg = MathConfig {
            single_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_math_regions(s, &[], &[], &[], &ih, cfg);
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
        let (regs, _) = scan_math_regions(s, &[], &[], &[], &[], cfg);
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
        let (regs, _) = scan_math_regions(s, &[], &[], &[], &[], cfg);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], "$a + b$");
    }

    #[test]
    fn region_with_subscripts_and_emphasis_chars() {
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

    #[test]
    fn environment_basic() {
        let s = "before \\begin{align} x &= y \\end{align} after";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        let span = &s[regs[0].range.clone()];
        assert!(span.starts_with("\\begin{align}"));
        assert!(span.ends_with("\\end{align}"));
    }

    #[test]
    fn environment_nested_same_name() {
        let s = "\\begin{matrix} a \\begin{matrix} b \\end{matrix} c \\end{matrix}";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], s);
    }

    #[test]
    fn environment_starred_name() {
        let s = "\\begin{align*} x \\end{align*}";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
    }

    #[test]
    fn environment_custom_name_round_trips() {
        let s = "\\begin{widget} q \\end{widget}";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
    }

    #[test]
    fn environment_unbalanced_emits_error() {
        let s = "\\begin{align} x = 1 \n";
        let (regs, errs) = scan(s);
        assert!(regs.is_empty());
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], MathError::UnbalancedEnv { name, .. } if name == "align"));
    }

    #[test]
    fn environment_inside_display_is_one_region() {
        let s = "\\[ \\begin{aligned} a &= b \\end{aligned} \\]";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
    }
}
