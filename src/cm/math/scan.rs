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
//! Each recognised region carries a [`MathSpan`] tag (inline, display,
//! or environment) with the body byte range; the pretty-printer
//! ([`super::pretty`]) dispatches on it.
//!
//! Unmatched openers become [`MathError`] values without aborting the
//! scan. Brace imbalance inside a recognised body is checked once per
//! region and surfaces as [`MathError::UnbalancedBraces`]; the region
//! still scans (its markers are balanced) but the pretty-printer
//! falls back to verbatim emission.

use std::ops::Range;

use crate::ir::{CodeBlock, HtmlBlock, InlineCode, InlineHtml};

use super::MathRegion;
use super::env::{EnvKind, KnownEnv};
use super::span::{AnyDelim, DisplayDelim, InlineDelim, MathError, MathSpan};

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
/// (non-overlapping) and any unmatched openers / brace-imbalanced
/// bodies as errors.
#[tracing::instrument(level = "debug", skip_all, fields(len = source.len()))]
pub(crate) fn scan_math_regions(
    source: &str,
    inline_codes: &[InlineCode],
    code_blocks: &[CodeBlock],
    html_blocks: &[HtmlBlock],
    inline_html: &[InlineHtml],
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
            && let Some((env_name, name_range, after_begin)) = match_begin(source, bytes, i)
        {
            match find_end_env(source, bytes, after_begin, env_name, &exclusions) {
                Some((end_start, end_after)) => {
                    let region = i..end_after;
                    let body = after_begin..end_start;
                    let env = match KnownEnv::from_name(env_name) {
                        Some(k) => EnvKind::Known(k),
                        None => EnvKind::Custom(name_range),
                    };
                    let span = MathSpan::Environment {
                        env,
                        body: body.clone(),
                    };
                    record_brace_errors(source, &region, &body, &mut errors);
                    regions.push(MathRegion {
                        range: region.clone(),
                        span,
                    });
                    tracing::debug!(env = env_name, range = ?region, "env region");
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
                let region = i..region_end;
                let body = content_start..close_start;
                let span = match delim {
                    AnyDelim::Paren => MathSpan::Inline {
                        delim: InlineDelim::Paren,
                        body: body.clone(),
                    },
                    AnyDelim::Dollar => MathSpan::Inline {
                        delim: InlineDelim::Dollar,
                        body: body.clone(),
                    },
                    AnyDelim::Bracket => MathSpan::Display {
                        delim: DisplayDelim::Bracket,
                        body: body.clone(),
                    },
                    AnyDelim::Dollar2 => MathSpan::Display {
                        delim: DisplayDelim::Dollar2,
                        body: body.clone(),
                    },
                };
                record_brace_errors(source, &region, &body, &mut errors);
                regions.push(MathRegion {
                    range: region.clone(),
                    span,
                });
                tracing::debug!(delim = delim.open(), range = ?region, "delim region");
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

/// Push a `MathError::UnbalancedBraces` if `body` (a sub-range of
/// `source`) has unbalanced `{` / `}`. Delegates to the shared
/// validator in [`super::pretty::body_braces_balanced`] so the
/// scanner and the pretty-printer agree on what counts as balanced.
fn record_brace_errors(
    source: &str,
    region: &Range<usize>,
    body: &Range<usize>,
    errors: &mut Vec<MathError>,
) {
    let Some(slice) = source.get(body.clone()) else {
        return;
    };
    if let Err(local_offset) = super::pretty::body_braces_balanced(slice) {
        errors.push(MathError::UnbalancedBraces {
            offset: body.start.saturating_add(local_offset),
            region: region.clone(),
        });
    }
}

/// Sorted, merged byte ranges where math regions cannot start or
/// extend through.
fn build_exclusions(
    inline_codes: &[InlineCode],
    code_blocks: &[CodeBlock],
    html_blocks: &[HtmlBlock],
    inline_html: &[InlineHtml],
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

/// Match `\begin{name}` at `i`. Returns `(name, byte range of the
/// name, position after the closing `}`)`. The `\` must not be itself
/// escaped (even-count of preceding backslashes).
fn match_begin<'a>(
    source: &'a str,
    bytes: &[u8],
    i: usize,
) -> Option<(&'a str, Range<usize>, usize)> {
    let after = match_kw(bytes, i, b"begin")?;
    parse_env_name(source, after)
}

/// Match `\end{name}` at `j`. Returns `(name, byte range of the name,
/// position after the closing `}`)`.
fn match_end<'a>(
    source: &'a str,
    bytes: &[u8],
    j: usize,
) -> Option<(&'a str, Range<usize>, usize)> {
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
/// (LaTeX environment name convention). Returns `(name, byte range of
/// the name in `source`, position after the closing `}`)`.
fn parse_env_name(source: &str, after: usize) -> Option<(&str, Range<usize>, usize)> {
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
    Some((name, name_start..j, j.saturating_add(1)))
}

/// Find the matching `\end{name}` for an open environment. Returns
/// the byte index of the `\` of `\end` and the byte index just after
/// the closing `}` of `\end{name}`. Counts nested `\begin{name}` so
/// inner environments of the same name do not close the outer.
fn find_end_env(
    source: &str,
    bytes: &[u8],
    from: usize,
    name: &str,
    exclusions: &[Range<usize>],
) -> Option<(usize, usize)> {
    let mut depth: u32 = 1;
    let mut j = from;
    while j < bytes.len() {
        if let Some(end) = excluded_end(exclusions, j) {
            j = end;
            continue;
        }
        if let Some((found_name, _, after)) = match_end(source, bytes, j) {
            if found_name == name {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((j, after));
                }
            }
            j = after;
            continue;
        }
        if let Some((found_name, _, after)) = match_begin(source, bytes, j) {
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
        assert!(matches!(
            regs[0].span,
            MathSpan::Display {
                delim: DisplayDelim::Bracket,
                ..
            }
        ));
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
        assert!(matches!(
            regs[0].span,
            MathSpan::Inline {
                delim: InlineDelim::Paren,
                ..
            }
        ));
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
            MathError::UnbalancedEnv { .. } | MathError::UnbalancedBraces { .. } => {
                panic!("expected delim error")
            }
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
        assert!(matches!(
            regs[0].span,
            MathSpan::Display {
                delim: DisplayDelim::Dollar2,
                ..
            }
        ));
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
        assert!(matches!(
            regs[0].span,
            MathSpan::Inline {
                delim: InlineDelim::Dollar,
                ..
            }
        ));
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
        match &regs[0].span {
            MathSpan::Environment { env, body } => {
                assert!(matches!(env, EnvKind::Known(KnownEnv::Align)));
                // body starts after `\begin{align}` (13 bytes) plus the
                // 7-byte prefix `before `.
                assert_eq!(body.start, 7 + 13);
                assert!(&s[body.clone()].contains("x &= y"));
            }
            MathSpan::Inline { .. } | MathSpan::Display { .. } => {
                panic!("expected environment span")
            }
        }
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
        assert!(matches!(
            &regs[0].span,
            MathSpan::Environment {
                env: EnvKind::Known(KnownEnv::AlignStar),
                ..
            }
        ));
    }

    #[test]
    fn environment_custom_name_round_trips() {
        let s = "\\begin{widget} q \\end{widget}";
        let regs = regions(s);
        assert_eq!(regs.len(), 1);
        match &regs[0].span {
            MathSpan::Environment {
                env: EnvKind::Custom(name_range),
                ..
            } => {
                assert_eq!(&s[name_range.clone()], "widget");
            }
            MathSpan::Inline { .. }
            | MathSpan::Display { .. }
            | MathSpan::Environment {
                env: EnvKind::Known(_),
                ..
            } => {
                panic!("expected custom env")
            }
        }
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
        // The outer region is Display (brackets); the inner aligned
        // is part of the body, not its own top-level region.
        assert!(matches!(
            &regs[0].span,
            MathSpan::Display {
                delim: DisplayDelim::Bracket,
                ..
            }
        ));
    }

    #[test]
    fn brace_imbalance_emits_error_but_region_still_scans() {
        let s = r"\[ \frac{a}{b \]";
        let (regs, errs) = scan(s);
        assert_eq!(regs.len(), 1);
        assert!(
            errs.iter()
                .any(|e| matches!(e, MathError::UnbalancedBraces { .. }))
        );
    }

    #[test]
    fn brace_balance_with_escaped_braces() {
        let s = r"\[ \{ a \} \]";
        let (_, errs) = scan(s);
        assert!(
            errs.iter()
                .all(|e| !matches!(e, MathError::UnbalancedBraces { .. })),
            "escaped braces should not count: {errs:?}"
        );
    }
}
