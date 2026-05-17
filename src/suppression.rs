//! Suppression-comment filter for diagnostics.
//!
//! Private module: built once at the start of `Document::lint_with`,
//! consulted once per emitted diagnostic, dropped at function exit.
//! The map is the single point where the parsed [`Suppression`]
//! directives become "which rule is silent over which byte range".
//! No rule code knows the map exists.
//!
//! Block-level "next block" resolution is approximated by "from the
//! end of the comment line to the next blank line (or EOF)". This
//! matches the `CommonMark` paragraph boundary in well-formatted
//! Markdown and matches what users expect from
//! `<!-- mdwright: allow ... -->`. Pulldown-cmark does not surface
//! paragraph ranges directly; the blank-line scan keeps this module
//! independent of an IR change.
//!
//! [`Suppression`]: crate::Suppression

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use crate::diagnostic::Diagnostic;
use crate::ir::{AllowScope, Ir, SuppressionKind};

pub(crate) struct SuppressionMap {
    /// Per-rule suppressed byte regions. Range membership is checked
    /// by `Range::contains(&span.start)` — i.e., we suppress if the
    /// diagnostic's first byte falls inside any region for its rule.
    by_rule: HashMap<String, Vec<Range<usize>>>,
    /// Regions where *every* rule is suppressed (parsed from
    /// `disable-all` / bare `disable`).
    all: Vec<Range<usize>>,
}

impl SuppressionMap {
    /// Build the map from a parsed IR plus the set of rule names
    /// known to the dispatcher. Returns the map together with one
    /// advisory `suppression` diagnostic per unknown rule name found
    /// in the IR's suppression comments — the dispatcher folds these
    /// into its output so users see syntax mistakes the same way they
    /// see other lints.
    pub(crate) fn build(ir: &Ir<'_>, known_rules: &[&str]) -> (Self, Vec<Diagnostic>) {
        let mut by_rule: HashMap<String, Vec<Range<usize>>> = HashMap::new();
        let mut all: Vec<Range<usize>> = Vec::new();
        let mut unknown: Vec<Diagnostic> = Vec::new();

        // Track per-rule and global open regions while walking
        // suppressions in source order. Regions close at the next
        // matching `enable` or at EOF.
        let mut open_per_rule: HashMap<String, usize> = HashMap::new();
        let mut open_all: Option<usize> = None;

        let mut sorted: Vec<&crate::ir::Suppression<'_>> = ir.suppressions.iter().collect();
        sorted.sort_by_key(|s| s.raw_range.start);

        for sup in sorted {
            // Surface unknown names as advisory diagnostics. The
            // suppression itself still applies — a typo silences
            // nothing but doesn't break anything else either.
            for name in &sup.rules {
                if !known_rules.contains(name)
                    && let Ok((line, column)) =
                        ir.line_index.locate(ir.source, sup.raw_range.start)
                {
                    unknown.push(Diagnostic {
                        rule: Cow::Borrowed("suppression"),
                        line,
                        column,
                        span: sup.raw_range.clone(),
                        message: format!("unknown rule '{name}' in mdwright suppression"),
                        fix: None,
                        advisory: true,
                    });
                }
            }

            match sup.kind {
                SuppressionKind::Allow { scope } => {
                    let span = match scope {
                        AllowScope::Block => next_block_span(ir.source, sup.raw_range.end),
                        AllowScope::NextLine => next_line_span(ir.source, sup.raw_range.end),
                    };
                    let Some(span) = span else { continue };
                    for name in &sup.rules {
                        by_rule
                            .entry((*name).to_owned())
                            .or_default()
                            .push(span.clone());
                    }
                }
                SuppressionKind::Disable => {
                    let start = sup.raw_range.end;
                    if sup.rules.is_empty() {
                        if open_all.is_none() {
                            open_all = Some(start);
                        }
                    } else {
                        for name in &sup.rules {
                            open_per_rule.entry((*name).to_owned()).or_insert(start);
                        }
                    }
                }
                SuppressionKind::Enable => {
                    let end = sup.raw_range.start;
                    if sup.rules.is_empty() {
                        if let Some(start) = open_all.take() {
                            all.push(start..end);
                        }
                    } else {
                        for name in &sup.rules {
                            if let Some(start) = open_per_rule.remove(*name) {
                                by_rule
                                    .entry((*name).to_owned())
                                    .or_default()
                                    .push(start..end);
                            }
                        }
                    }
                }
            }
        }

        // Close any still-open regions at EOF.
        let eof = ir.source.len();
        if let Some(start) = open_all {
            all.push(start..eof);
        }
        for (name, start) in open_per_rule {
            by_rule.entry(name).or_default().push(start..eof);
        }

        (Self { by_rule, all }, unknown)
    }

    /// Is `rule` suppressed at the byte position `span.start`?
    pub(crate) fn suppresses(&self, rule: &str, span: &Range<usize>) -> bool {
        let probe = span.start;
        if self.all.iter().any(|r| r.contains(&probe)) {
            return true;
        }
        self.by_rule
            .get(rule)
            .is_some_and(|ranges| ranges.iter().any(|r| r.contains(&probe)))
    }
}

/// Byte range of the "next block" starting at or after `after`:
/// from the first non-blank byte to the start of the next blank
/// line (or EOF).
fn next_block_span(source: &str, after: usize) -> Option<Range<usize>> {
    let bytes = source.as_bytes();
    let mut cursor = after;
    while let Some(&b) = bytes.get(cursor) {
        if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
            break;
        }
        cursor = cursor.saturating_add(1);
    }
    if cursor >= bytes.len() {
        return None;
    }
    let start = cursor;
    let mut line_start = start;
    while line_start < bytes.len() {
        let mut line_end = line_start;
        while let Some(&b) = bytes.get(line_end) {
            if b == b'\n' {
                break;
            }
            line_end = line_end.saturating_add(1);
        }
        let line = source.get(line_start..line_end)?;
        let is_blank = line.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r'));
        if is_blank && line_start > start {
            return Some(start..line_start);
        }
        line_start = line_end.saturating_add(1);
    }
    Some(start..bytes.len())
}

/// Byte range of the next non-empty source line at or after `after`.
/// Handles pulldown-cmark's two possible `HtmlBlock` end positions
/// uniformly: whether `after` is at the trailing '\n' of the comment
/// line or just past it, the returned span covers the same following
/// line.
fn next_line_span(source: &str, after: usize) -> Option<Range<usize>> {
    let bytes = source.as_bytes();
    let mut line_start = after;
    loop {
        if line_start >= bytes.len() {
            return None;
        }
        let at_line_start =
            line_start == 0 || bytes.get(line_start.saturating_sub(1)).copied() == Some(b'\n');
        if at_line_start {
            break;
        }
        line_start = line_start.saturating_add(1);
    }
    let mut line_end = line_start;
    while let Some(&b) = bytes.get(line_end) {
        if b == b'\n' {
            break;
        }
        line_end = line_end.saturating_add(1);
    }
    Some(line_start..line_end)
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow};

    use super::{next_block_span, next_line_span};

    #[test]
    fn next_block_to_blank_line() -> Result<()> {
        let src = "comment line\n# Heading\nbody body\n\nnext block\n";
        let span = next_block_span(src, 13).ok_or_else(|| anyhow!("no span"))?;
        let got = src.get(span).ok_or_else(|| anyhow!("bad range"))?;
        assert_eq!(got, "# Heading\nbody body\n");
        Ok(())
    }

    #[test]
    fn next_block_at_eof() -> Result<()> {
        let src = "only one block\n";
        let span = next_block_span(src, 0).ok_or_else(|| anyhow!("no span"))?;
        let got = src.get(span).ok_or_else(|| anyhow!("bad range"))?;
        // No blank line means the block extends to EOF including the
        // final newline.
        assert_eq!(got, "only one block\n");
        Ok(())
    }

    #[test]
    fn next_line_at_line_start() -> Result<()> {
        let src = "header\nbody body\ntrailer\n";
        // `after` already points at the start of "body body".
        let span = next_line_span(src, 7).ok_or_else(|| anyhow!("no span"))?;
        let got = src.get(span).ok_or_else(|| anyhow!("bad range"))?;
        assert_eq!(got, "body body");
        Ok(())
    }

    #[test]
    fn next_line_mid_line_skips_forward() -> Result<()> {
        let src = "X\nyy\nzz\n";
        // `after = 0` is mid-line; the first start-of-line at or after
        // 0 is byte 0 itself. The line starting at 0 is "X".
        let span = next_line_span(src, 0).ok_or_else(|| anyhow!("no span"))?;
        let got = src.get(span).ok_or_else(|| anyhow!("bad range"))?;
        assert_eq!(got, "X");
        Ok(())
    }

    #[test]
    fn next_line_from_newline_byte() -> Result<()> {
        // After byte 1 (the '\n' after 'X'): the first start-of-line
        // is byte 2, line "yy".
        let src = "X\nyy\nzz\n";
        let span = next_line_span(src, 1).ok_or_else(|| anyhow!("no span"))?;
        let got = src.get(span).ok_or_else(|| anyhow!("bad range"))?;
        assert_eq!(got, "yy");
        Ok(())
    }
}
