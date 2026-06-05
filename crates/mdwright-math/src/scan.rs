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
//! or environment) with the body byte range.
//!
//! Unmatched openers become [`MathError`] values without aborting the
//! scan. Brace imbalance inside a recognised body is checked once per
//! region and surfaces as [`MathError::UnbalancedBraces`]; the region
//! still scans because its markers are balanced; canonicalisation
//! skips body rewrites for that region.

use std::ops::Range;

use super::MathRegion;
use super::env::{EnvKind, KnownEnv};
use super::span::{AnyDelim, DisplayDelim, InlineDelim, MathBody, MathError, MathSpan};

/// Which math delimiter pairs to recognise.
///
/// Defaults recognise `\(…\)`, `\[…\]`, and LaTeX environments. The
/// dollar variants are opt-in because `$` collides with currency
/// symbols and shell prompts in non-math prose.
#[derive(Copy, Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct MathConfig {
    pub backslash_bracket: bool,
    pub backslash_paren: bool,
    pub double_dollar: bool,
    pub single_dollar: bool,
    /// LaTeX `\begin{env}…\end{env}` recognition. Defaults to `true`:
    /// environments outside `\[ \]` are common in mathematical prose
    /// (e.g. raw `\begin{align}` blocks rendered by `KaTeX`) and have
    /// unambiguous closers, unlike `$`.
    pub environments: bool,
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
///
/// `transparent_runs` is a sorted, non-overlapping slice of byte
/// ranges the lexer must treat as if they do not exist — blockquote
/// `>` markers and list-item continuation indentation on continuation
/// lines. Math regions may cross transparent runs (they are not
/// region boundaries the way exclusion zones are); the runs are
/// recorded on each body so [`MathBody::as_str`] can yield clean
/// math content.
#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(len = source.len(), transparent = transparent_runs.len()),
)]
pub fn scan_math_regions(
    source: &str,
    exclusions: &[Range<usize>],
    transparent_runs: &[Range<usize>],
    cfg: MathConfig,
) -> (Vec<MathRegion>, Vec<MathError>) {
    let bytes = source.as_bytes();
    let mut regions: Vec<MathRegion> = Vec::new();
    let mut errors: Vec<MathError> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(end) = excluded_end(exclusions, i) {
            i = end;
            continue;
        }
        if let Some(end) = transparent_end(transparent_runs, i) {
            i = end;
            continue;
        }
        // Environments first: `\begin{name}` is structurally
        // unambiguous and would otherwise be passed over.
        if cfg.environments
            && let Some((env_name, name_range, after_begin)) = match_begin(source, bytes, i)
        {
            match find_end_env(source, bytes, after_begin, env_name, exclusions, transparent_runs) {
                Some((end_start, end_after)) => {
                    let region = i..end_after;
                    let body_range = after_begin..end_start;
                    let env = match KnownEnv::from_name(env_name) {
                        Some(k) => EnvKind::Known(k),
                        None => EnvKind::Custom(name_range),
                    };
                    let body = build_math_body(body_range.clone(), transparent_runs);
                    record_brace_errors(source, &region, &body, &mut errors);
                    let span = MathSpan::Environment { env, body };
                    regions.push(MathRegion::new(region.clone(), span));
                    tracing::debug!(
                        env = env_name,
                        range = ?region,
                        stripped = !body_runs_empty(&body_range, transparent_runs),
                        "env region",
                    );
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
        match find_close(bytes, content_start, delim, exclusions, transparent_runs) {
            Some(close_start) => {
                // Reject backslash-delimited bodies with no alphanumeric
                // content. A `\(...\)` or `\[...\]` whose body is only
                // backslashes, brackets, or whitespace is almost certainly
                // a sequence of CM backslash escapes, not math — GFM §6.1
                // ex. 308's `\!\"...\(\)...\[\\\]...` is the canonical case.
                // Without this guard the recogniser would treat `\(\)`
                // (empty) and `\[\\\]` (body `\\`) as math, the formatter
                // would normalise them, and the round-trip HTML would
                // diverge from the source's escape-sequence rendering.
                //
                // This guard is scoped to `\[`/`\(` deliberately: `$` and
                // `$$` cannot arise from CommonMark escape sequences, so a
                // symbol-only body like `$+$`, `$<$`, or `$[-,-]$` is real
                // math. Applying the guard to dollars used to skip the
                // opener but leave the closing `$` to be re-scanned as a
                // fresh opener, yielding a spurious `UnbalancedDelim`.
                let body_slice = bytes.get(content_start..close_start).unwrap_or(&[]);
                if matches!(delim, AnyDelim::Bracket | AnyDelim::Paren)
                    && !body_slice.iter().any(u8::is_ascii_alphanumeric)
                {
                    i = i.saturating_add(1);
                    continue;
                }
                let close_len = delim.close().len();
                let region_end = close_start.saturating_add(close_len);
                let region = i..region_end;
                let body_range = content_start..close_start;
                let body = build_math_body(body_range.clone(), transparent_runs);
                record_brace_errors(source, &region, &body, &mut errors);
                let span = match delim {
                    AnyDelim::Paren => MathSpan::Inline {
                        delim: InlineDelim::Paren,
                        body,
                    },
                    AnyDelim::Dollar => MathSpan::Inline {
                        delim: InlineDelim::Dollar,
                        body,
                    },
                    AnyDelim::Bracket => MathSpan::Display {
                        delim: DisplayDelim::Bracket,
                        body,
                    },
                    AnyDelim::Dollar2 => MathSpan::Display {
                        delim: DisplayDelim::Dollar2,
                        body,
                    },
                };
                regions.push(MathRegion::new(region.clone(), span));
                tracing::debug!(
                    delim = delim.open(),
                    range = ?region,
                    stripped = !body_runs_empty(&body_range, transparent_runs),
                    "delim region",
                );
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

/// Collect the slice of transparent runs intersecting `body_range`
/// and pack them into a [`MathBody`]. The recogniser keeps the
/// invariant that `transparent_runs` is sorted and non-overlapping,
/// so the intersection is contiguous.
fn build_math_body(body_range: Range<usize>, transparent_runs: &[Range<usize>]) -> MathBody {
    let runs: Box<[Range<usize>]> = transparent_runs
        .iter()
        .filter(|r| r.start < body_range.end && body_range.start < r.end)
        .cloned()
        .collect();
    MathBody::new(body_range, runs)
}

/// True iff no transparent run intersects `body_range`. Cheap probe
/// for the tracing debug field — the actual `Box<[Range]>` allocation
/// happens once inside [`build_math_body`].
fn body_runs_empty(body_range: &Range<usize>, transparent_runs: &[Range<usize>]) -> bool {
    !transparent_runs
        .iter()
        .any(|r| r.start < body_range.end && body_range.start < r.end)
}

/// Push a `MathError::UnbalancedBraces` if `body`'s clean content has
/// unbalanced `{` / `}`. Delegates to the shared validator in
/// [`super::normalise::body_braces_balanced`] so the scanner, the
/// canonicalise math rewrite, and the lint rule agree on what counts
/// as balanced. The check runs over the clean body, so container
/// prefixes cannot affect brace balance, and the local offset is
/// mapped back to a source-absolute byte via
/// [`MathBody::clean_offset_to_source`].
fn record_brace_errors(source: &str, region: &Range<usize>, body: &MathBody, errors: &mut Vec<MathError>) {
    let clean = body.as_str(source);
    if let Err(local_offset) = super::normalise::body_braces_balanced(clean.as_ref()) {
        errors.push(MathError::UnbalancedBraces {
            offset: body.clean_offset_to_source(local_offset),
            region: region.clone(),
        });
    }
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

/// True iff `i` lies inside any transparent run, returning the run's
/// end. Mirrors [`excluded_end`] structurally but has different
/// semantics: transparent runs do not bound math regions; the scanner
/// and [`find_close`] / [`find_end_env`] use this to skip prefix
/// bytes while keeping the surrounding region intact.
fn transparent_end(transparent_runs: &[Range<usize>], i: usize) -> Option<usize> {
    let idx = transparent_runs.partition_point(|r| r.start <= i);
    if let Some(prev_idx) = idx.checked_sub(1)
        && let Some(r) = transparent_runs.get(prev_idx)
        && i < r.end
    {
        return Some(r.end);
    }
    None
}

/// Match `\begin{name}` at `i`. Returns `(name, byte range of the
/// name, position after the closing `}`)`. The `\` must not be itself
/// escaped (even-count of preceding backslashes).
fn match_begin<'a>(source: &'a str, bytes: &[u8], i: usize) -> Option<(&'a str, Range<usize>, usize)> {
    let after = match_kw(bytes, i, b"begin")?;
    parse_env_name(source, after)
}

/// Match `\end{name}` at `j`. Returns `(name, byte range of the name,
/// position after the closing `}`)`.
fn match_end<'a>(source: &'a str, bytes: &[u8], j: usize) -> Option<(&'a str, Range<usize>, usize)> {
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
    transparent_runs: &[Range<usize>],
) -> Option<(usize, usize)> {
    let mut depth: u32 = 1;
    let mut j = from;
    while j < bytes.len() {
        if let Some(end) = excluded_end(exclusions, j) {
            j = end;
            continue;
        }
        if let Some(end) = transparent_end(transparent_runs, j) {
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
            // `\$` is a CommonMark-escaped literal dollar, not a delimiter.
            if !preceding_backslashes_even(bytes, i) {
                return None;
            }
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
    transparent_runs: &[Range<usize>],
) -> Option<usize> {
    let mut j = from;
    while j < bytes.len() {
        if excluded_end(exclusions, j).is_some() {
            // Math regions don't cross an exclusion boundary.
            return None;
        }
        if let Some(end) = transparent_end(transparent_runs, j) {
            // Transparent bytes (container prefixes) are not part of
            // the math content; skip past them and keep looking for
            // the close.
            j = end;
            continue;
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
                    && preceding_backslashes_even(bytes, j)
                {
                    return Some(j);
                }
            }
            AnyDelim::Dollar => {
                if bytes.get(j).copied() == Some(b'$') && preceding_backslashes_even(bytes, j) {
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
    use std::borrow::Cow;

    use super::*;

    fn scan(source: &str) -> (Vec<MathRegion>, Vec<MathError>) {
        scan_math_regions(source, &[], &[], MathConfig::default())
    }

    fn scan_with_runs(
        source: &str,
        transparent_runs: &[Range<usize>],
        cfg: MathConfig,
    ) -> (Vec<MathRegion>, Vec<MathError>) {
        scan_math_regions(source, &[], transparent_runs, cfg)
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
            regs[0].span(),
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
            regs[0].span(),
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
    #[allow(
        clippy::single_range_in_vec_init,
        reason = "test intentionally passes one exclusion range"
    )]
    fn region_inside_code_span_excluded() {
        let s = r"text `\[ x \]` more";
        let exclusions = [5..14];
        let (regs, _) = scan_math_regions(s, &exclusions, &[], MathConfig::default());
        assert!(regs.is_empty());
    }

    #[test]
    #[allow(
        clippy::single_range_in_vec_init,
        reason = "test intentionally passes one exclusion range"
    )]
    fn region_inside_code_block_excluded() {
        let s = "```\n\\[ x \\]\n```";
        let exclusions = [0..s.len()];
        let (regs, _) = scan_math_regions(s, &exclusions, &[], MathConfig::default());
        assert!(regs.is_empty());
    }

    #[test]
    #[allow(
        clippy::single_range_in_vec_init,
        reason = "test intentionally passes one exclusion range"
    )]
    fn region_inside_inline_html_excluded() {
        let s = r#"see <a href="/x?val=$foo">x</a> after"#;
        let exclusions = [4..26];
        let cfg = MathConfig {
            single_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_math_regions(s, &exclusions, &[], cfg);
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
        let (regs, _) = scan_math_regions(s, &[], &[], cfg);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], "$$ x = 5 $$");
        assert!(matches!(
            regs[0].span(),
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
        let (regs, _) = scan_math_regions(s, &[], &[], cfg);
        assert_eq!(regs.len(), 1);
        assert_eq!(&s[regs[0].range.clone()], "$a + b$");
        assert!(matches!(
            regs[0].span(),
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
        match regs[0].span() {
            MathSpan::Environment { env, body } => {
                assert!(matches!(env, EnvKind::Known(KnownEnv::Align)));
                assert!(body.as_str(s).contains("x &= y"));
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
            regs[0].span(),
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
        match regs[0].span() {
            MathSpan::Environment {
                env: EnvKind::Custom(name_range),
                ..
            } => {
                assert_eq!(&s[name_range.clone()], "widget");
            }
            MathSpan::Inline { .. }
            | MathSpan::Display { .. }
            | MathSpan::Environment {
                env: EnvKind::Known(_), ..
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
            regs[0].span(),
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
        assert!(errs.iter().any(|e| matches!(e, MathError::UnbalancedBraces { .. })));
    }

    #[test]
    fn brace_balance_with_escaped_braces() {
        let s = r"\[ \{ a \} \]";
        let (_, errs) = scan(s);
        assert!(
            errs.iter().all(|e| !matches!(e, MathError::UnbalancedBraces { .. })),
            "escaped braces should not count: {errs:?}"
        );
    }

    #[test]
    fn transparent_run_in_blockquote_strips_prefix() {
        // `> $$\n> x = 1\n> $$` — the `> ` on lines 2 and 3 sits
        // inside the math body and must be stripped from the clean
        // content.
        let s = "> $$\n> x = 1\n> $$";
        // The `> ` prefix sits on each line. The math open is at
        // byte 2 (`$$`). The body runs from byte 4 (after `$$`) to
        // byte 15 (before the close `$$`). Transparent runs cover
        // the `> ` prefixes on lines 2 and 3.
        let runs = vec![5..7, 13..15];
        let cfg = MathConfig {
            double_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_with_runs(s, &runs, cfg);
        assert_eq!(regs.len(), 1, "expected one region in {s:?}");
        let body = regs[0].span().body();
        let clean = body.as_str(s);
        assert!(
            matches!(&clean, Cow::Owned(_)),
            "expected owned body for container-nested math, got {clean:?}",
        );
        assert!(!clean.contains('>'), "container prefix leaked: {clean:?}");
        assert!(clean.contains("x = 1"), "body lost content: {clean:?}");
    }

    #[test]
    fn transparent_run_in_list_item_strips_indent() {
        // `1. item\n   $$\n   x = 1\n   $$` — the 3-space
        // continuation indent on lines 2-4 must be stripped from the
        // clean content.
        let s = "1. item\n   $$\n   x = 1\n   $$";
        // Continuation indents at line 2 (bytes 8..11), line 3
        // (14..17), line 4 (23..26).
        let runs = vec![8..11, 14..17, 23..26];
        let cfg = MathConfig {
            double_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_with_runs(s, &runs, cfg);
        assert_eq!(regs.len(), 1);
        let clean = regs[0].span().body().as_str(s);
        assert!(matches!(&clean, Cow::Owned(_)));
        assert!(!clean.contains("   "), "indent leaked: {clean:?}");
        assert!(clean.contains("x = 1"));
    }

    #[test]
    fn nested_blockquote_combined_prefix() {
        // `> > $$\n> > x\n> > $$` — both nesting levels' `> `
        // prefixes are combined into one transparent run per line.
        let s = "> > $$\n> > x\n> > $$";
        // Line 2 (`> > x`): bytes 7..11 → "> > " (4 bytes).
        // Line 3 (`> > $$`): bytes 13..17 → "> > " (4 bytes).
        let runs = vec![7..11, 13..17];
        let cfg = MathConfig {
            double_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_with_runs(s, &runs, cfg);
        assert_eq!(regs.len(), 1);
        let clean = regs[0].span().body().as_str(s);
        assert!(!clean.contains('>'), "prefix leaked: {clean:?}");
        assert!(clean.contains('x'));
    }

    #[test]
    fn top_level_math_borrows() {
        // `$$\nx\n$$` at root with empty transparent runs: the body
        // is borrowed from source, no allocation. Test the
        // `Cow::Borrowed` discriminant explicitly so the fast path
        // can't regress silently.
        let s = "$$\nx\n$$";
        let cfg = MathConfig {
            double_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_with_runs(s, &[], cfg);
        assert_eq!(regs.len(), 1);
        let clean = regs[0].span().body().as_str(s);
        assert!(
            matches!(clean, Cow::Borrowed(_)),
            "expected borrowed body for top-level math",
        );
    }

    #[test]
    fn body_source_ranges_can_drive_latex_translation_without_markdown_parsing() {
        let s = r"Inline \( \alpha_i \) and \[ x^{2} \].";
        let regs = regions(s);
        let ranges = regs
            .iter()
            .map(|region| region.span().body().source_range())
            .collect::<Vec<_>>();

        let translated = mdwright_latex::translate_latex_ranges_to_unicode(s, &ranges);

        assert_eq!(translated.text(), r"Inline \( αᵢ \) and \[ x² \].");
        assert_eq!(translated.edit_count(), 2);
        assert!(translated.is_lossless());
    }

    #[test]
    fn transparent_run_protects_delim_match() {
        // `> $$ x\n> $$` — the close `$$` on line 2 is outside any
        // transparent run; the region must still be recognised
        // end-to-end with the body crossing the `\n> ` prefix.
        let s = "> $$ x\n> $$";
        let run = 7..9;
        let runs = std::slice::from_ref(&run);
        let cfg = MathConfig {
            double_dollar: true,
            ..MathConfig::default()
        };
        let (regs, _) = scan_with_runs(s, runs, cfg);
        assert_eq!(regs.len(), 1, "expected one region in {s:?}");
        // Close $$ is at bytes 9..11.
        assert_eq!(regs[0].range.end, 11);
    }

    #[test]
    fn transparent_run_blocks_spurious_delim() {
        // `not math\n> $\n` with single-dollar enabled and the `> `
        // recorded as a transparent run on line 2. The `$` after
        // the prefix is at the end of the line and never sees a
        // close — so the scanner records an UnbalancedDelim, not
        // a recognised region. The point of the test: the lexer
        // does NOT see a `$` inside the transparent prefix bytes
        // themselves (no spurious region anchored at `>`).
        let s = "not math\n> $\n";
        let run = 9..11;
        let runs = std::slice::from_ref(&run);
        let cfg = MathConfig {
            single_dollar: true,
            ..MathConfig::default()
        };
        let (regs, errs) = scan_with_runs(s, runs, cfg);
        assert!(regs.is_empty(), "no region should match in {s:?}");
        assert!(
            errs.iter().any(|e| matches!(e, MathError::UnbalancedDelim { .. })),
            "expected an UnbalancedDelim for the unclosed `$`: {errs:?}",
        );
    }

    fn dollar_cfg() -> MathConfig {
        MathConfig {
            single_dollar: true,
            double_dollar: true,
            ..MathConfig::default()
        }
    }

    #[test]
    fn symbol_only_dollar_body_is_recognised() {
        // `$+$`, `$<$`, `$[-,-]$` are balanced inline math whose body has
        // no alphanumeric. They must be recognised (one region, no error):
        // the old guard skipped the opener and re-scanned the closing `$`
        // as a fresh opener, producing a spurious UnbalancedDelim.
        for s in ["a $+$ b", "a $<$ b", "a $[-,-]$ b"] {
            let (regs, errs) = scan_with_runs(s, &[], dollar_cfg());
            assert_eq!(regs.len(), 1, "expected one region in {s:?}");
            assert!(errs.is_empty(), "unexpected errors in {s:?}: {errs:?}");
            assert!(matches!(
                regs[0].span(),
                MathSpan::Inline {
                    delim: InlineDelim::Dollar,
                    ..
                }
            ));
        }
    }

    #[test]
    fn escaped_dollar_is_not_a_delimiter() {
        // `\$` is a CommonMark literal dollar; a lone one is not math.
        let s = r"a lone \$ sign";
        let (regs, errs) = scan_with_runs(s, &[], dollar_cfg());
        assert!(regs.is_empty(), "no region in {s:?}: {regs:?}");
        assert!(errs.is_empty(), "no errors in {s:?}: {errs:?}");
    }

    #[test]
    fn escaped_dollar_inside_body_does_not_close() {
        // The mid-body `\$` is literal, so the region runs to the final
        // unescaped `$`.
        let s = r"x $a \$ b$ y";
        let (regs, errs) = scan_with_runs(s, &[], dollar_cfg());
        assert_eq!(regs.len(), 1, "expected one region in {s:?}");
        assert!(errs.is_empty(), "no errors in {s:?}: {errs:?}");
        assert_eq!(&s[regs[0].range.clone()], r"$a \$ b$");
    }

    #[test]
    fn backslash_escape_noise_still_rejected() {
        // GFM §6.1 ex. 308: `\[\\\]` is escape noise, not math. The bracket
        // guard must still reject it without a spurious delim error.
        let s = r"x \[\\\] y";
        let (regs, errs) = scan(s);
        assert!(regs.is_empty(), "no region in {s:?}: {regs:?}");
        assert!(
            !errs.iter().any(|e| matches!(e, MathError::UnbalancedDelim { .. })),
            "no spurious delim error in {s:?}: {errs:?}",
        );
    }
}
