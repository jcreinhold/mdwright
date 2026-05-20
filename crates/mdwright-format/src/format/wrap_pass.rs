//! Terminal paragraph wrap over normalized source bytes.
//!
//! Knuth-Plass-lite squared-slack DP targeting byte ranges.
//!
//! ## Contract
//!
//! [`collect_terminal_wrap_edits`] walks the document's cached paragraph
//! facts and rewrites each paragraph's bytes per [`Wrap`]:
//!
//! - `Wrap::Keep`: no-op (identity emit already preserved breaks).
//! - `Wrap::No`: collapse every soft break inside a paragraph to a
//!   single space; hard breaks (CM `\` or two-trailing-space) keep
//!   the line boundary.
//! - `Wrap::At(n)`: reflow each paragraph so no line exceeds `n`
//!   columns. Inline atomics (code spans, links, images, raw inline
//!   HTML, inline math) are unbreakable; hard breaks force a line
//!   boundary; container prefixes (`> ` for blockquotes, indent for
//!   list items) are preserved on every continuation line and shrink
//!   the per-line budget.
//!
//! ATX and setext headings are explicitly never reflowed (their
//! grammar forbids mid-syntax breaks). The pass touches paragraphs
//! only.
//!
//! ## Safety
//!
//! For each paragraph, the rewrite extracts inline atomics by source byte
//! range, tokenises the remaining text on whitespace, applies the DP, and
//! re-emits with `\n` + the continuation prefix between lines. The rewrite
//! engine calls this module only after earlier rewrite families have reached
//! local normal form for the current snapshot, then verifies the paragraph
//! batch in document context before commit.

use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthStr;

use crate::Wrap;
use crate::format::rewrite::{Candidate, OwnerKind, Snapshot, Verification};
use mdwright_document::StructuralKind;
use mdwright_document::WrappableParagraph as Paragraph;

/// Time budget per paragraph for the DP.
const MAX_WRAP_TIME: Duration = Duration::from_millis(100);
const TIME_CHECK_STRIDE: usize = 1 << 10;
const MAX_WRAP_TOKENS: usize = 100_000;
const OVERFLOW_PENALTY: u64 = 1_000_000;

pub(crate) struct TerminalWrapEdits {
    pub(crate) edits: Vec<Candidate>,
    pub(crate) skipped_unsupported: usize,
}

/// Collect terminal wrap edits for every paragraph in `out`. No-op
/// when `mode` is [`Wrap::Keep`].
pub(crate) fn collect_terminal_wrap_edits(snapshot: &Snapshot<'_>, mode: Wrap) -> TerminalWrapEdits {
    let mut collected = TerminalWrapEdits {
        edits: Vec::new(),
        skipped_unsupported: 0,
    };
    if matches!(mode, Wrap::Keep) {
        return collected;
    }
    let source = snapshot.source();
    let paragraphs = snapshot.document().wrappable_paragraphs();
    for p in paragraphs {
        let replacement = match rewrap_paragraph(source, p, mode) {
            ParagraphWrap::Edit(replacement) => replacement,
            ParagraphWrap::Noop => continue,
            ParagraphWrap::Unsupported => {
                collected.skipped_unsupported = collected.skipped_unsupported.saturating_add(1);
                continue;
            }
        };
        let line_range = p.line_range();
        let Some(existing) = source.get(line_range.clone()) else {
            collected.skipped_unsupported = collected.skipped_unsupported.saturating_add(1);
            continue;
        };
        if replacement == existing {
            continue;
        }
        if let Some(candidate) = snapshot.candidate(
            wrap_owner_kind(p.owner_kind()),
            line_range,
            replacement,
            Verification::PreserveMarkdownAndMath,
            "paragraph-wrap",
        ) {
            collected.edits.push(candidate);
        } else {
            collected.skipped_unsupported = collected.skipped_unsupported.saturating_add(1);
        }
    }
    collected
}

fn wrap_owner_kind(kind: StructuralKind) -> OwnerKind {
    match kind {
        StructuralKind::Paragraph => OwnerKind::Paragraph,
        StructuralKind::BlockQuote => OwnerKind::BlockQuote,
        StructuralKind::ListItem => OwnerKind::ListItem,
        StructuralKind::DefinitionDescription => OwnerKind::DefinitionDescription,
        StructuralKind::FootnoteDefinition => OwnerKind::FootnoteDefinition,
        StructuralKind::Heading
        | StructuralKind::List
        | StructuralKind::DefinitionList
        | StructuralKind::ThematicBreak
        | StructuralKind::Table => OwnerKind::Paragraph,
    }
}

enum ParagraphWrap {
    Noop,
    Edit(String),
    Unsupported,
}

/// Re-emit a paragraph per `mode`.
fn rewrap_paragraph(out: &str, p: &Paragraph, mode: Wrap) -> ParagraphWrap {
    let bytes = out.as_bytes();
    let content_range = p.content_range();
    let line_range = p.line_range();
    let content_hi = content_range.end;
    let Some(content) = bytes.get(content_range) else {
        return ParagraphWrap::Unsupported;
    };
    let trailing_suffix = if content_hi <= line_range.end {
        let Some(suffix) = bytes.get(content_hi..line_range.end) else {
            return ParagraphWrap::Unsupported;
        };
        if suffix.is_empty() && content.last().copied() == Some(b'\n') {
            "\n"
        } else {
            let Ok(suffix) = std::str::from_utf8(suffix) else {
                return ParagraphWrap::Unsupported;
            };
            suffix
        }
    } else {
        ""
    };
    let segments = split_at_hard_breaks(p, bytes);
    let first_prefix_width = display_width(p.first_prefix());
    let cont_prefix_width = display_width(p.cont_prefix());
    let target = match mode {
        Wrap::Keep => return ParagraphWrap::Noop,
        Wrap::No => u32::MAX,
        Wrap::At(n) => n.max(1),
    };

    let mut emitted = String::with_capacity(content.len().saturating_add(p.first_prefix().len()));
    emitted.push_str(p.first_prefix());

    let start = Instant::now();
    for (seg_idx, seg) in segments.iter().enumerate() {
        let Some(tokens) = tokenize_segment(bytes, seg, p.atomics(), p.cont_prefix()) else {
            return ParagraphWrap::Unsupported;
        };
        let first_target = if seg_idx == 0 {
            target.saturating_sub(first_prefix_width).max(1)
        } else {
            target.saturating_sub(cont_prefix_width).max(1)
        };
        let cont_target = target.saturating_sub(cont_prefix_width).max(1);
        // Non-terminal segments end with a hard-break marker (`\` or
        // `  `) appended to the last line; the marker has to fit
        // inside the wrap budget too. Shrink the per-segment last-
        // line target by the marker's display width.
        let last_line_extra = if seg_idx.saturating_add(1) < segments.len() {
            p.hard_breaks().get(seg_idx).map_or(0, |h| display_width(h.marker()))
        } else {
            0
        };
        let lines = if tokens.is_empty() {
            Vec::new()
        } else {
            match mode {
                Wrap::No => vec![tokens],
                Wrap::At(_) => {
                    let Some(lines) = layout_lines(&tokens, first_target, cont_target, last_line_extra, start) else {
                        return ParagraphWrap::Unsupported;
                    };
                    lines
                }
                Wrap::Keep => return ParagraphWrap::Noop,
            }
        };
        for (line_idx, line) in lines.iter().enumerate() {
            // The first line of segment 0 inherits the first-line
            // prefix that was already pushed before the loop. The
            // first line of each subsequent segment is positioned
            // by the hard-break terminator below. So we only insert
            // a line-break + cont-prefix when we're between two
            // soft-broken lines within the same segment.
            if line_idx > 0 {
                emitted.push('\n');
                emitted.push_str(p.cont_prefix());
            }
            for (k, tok) in line.iter().enumerate() {
                if k > 0 {
                    emitted.push(' ');
                }
                emitted.push_str(tok.text);
            }
        }
        // Hard break terminator: every non-final segment ends with
        // its hard-break marker, even when the segment has no
        // content (e.g. source `\\\nx` has an empty leading segment
        // before the hard break).
        if seg_idx.saturating_add(1) < segments.len() {
            let Some(hb) = p.hard_breaks().get(seg_idx) else {
                return ParagraphWrap::Unsupported;
            };
            emitted.push_str(hb.marker());
            emitted.push('\n');
            emitted.push_str(p.cont_prefix());
        }
    }
    emitted.push_str(trailing_suffix);
    if out.get(line_range).is_some_and(|existing| existing == emitted) {
        ParagraphWrap::Noop
    } else {
        ParagraphWrap::Edit(emitted)
    }
}

/// A wrap-token: either a verbatim slice of source bytes (atomic or
/// word).
struct Token<'a> {
    text: &'a str,
    width: u32,
}

fn split_at_hard_breaks(p: &Paragraph, bytes: &[u8]) -> Vec<Range<usize>> {
    let content_range = p.content_range();
    let hard_breaks = p.hard_breaks();
    let mut cuts: Vec<usize> = Vec::with_capacity(hard_breaks.len().saturating_add(1));
    let mut start = content_range.start;
    for hb in hard_breaks {
        if hb.marker_start() >= start && hb.newline() < content_range.end {
            cuts.push(start);
            start = hb.newline().saturating_add(1);
        }
    }
    cuts.push(start);
    // Build [start..end) segments from cuts.
    let mut segments: Vec<Range<usize>> = Vec::with_capacity(cuts.len());
    for (i, &lo) in cuts.iter().enumerate() {
        let hi = if i.saturating_add(1) < cuts.len() {
            // End of segment is the marker_lo of the next hard break.
            hard_breaks
                .get(i)
                .map_or(content_range.end, mdwright_document::ParagraphHardBreak::marker_start)
        } else {
            content_range.end
        };
        if hi > lo {
            segments.push(lo..hi);
        } else if hi == lo {
            // Empty segment after a hard break at content end is
            // legitimate; keep it so the trailing hard break still
            // emits.
            segments.push(lo..hi);
        }
    }
    // Suppress the trivial case where bytes is empty.
    if segments.is_empty() {
        segments.push(content_range);
    }
    let _ = bytes; // not currently used; kept for symmetry with future cases.
    segments
}

fn tokenize_segment<'a>(
    bytes: &'a [u8],
    seg: &Range<usize>,
    atomics: &[Range<usize>],
    cont_prefix: &str,
) -> Option<Vec<Token<'a>>> {
    // A token is a maximal byte slice not containing whitespace,
    // possibly spanning multiple atomic / non-atomic regions glued
    // together at byte-adjacency. The tokenizer walks bytes and
    // accumulates a current-token byte range; whitespace flushes
    // the token; atomic ranges extend it without splitting.
    //
    // Continuation prefix bytes immediately after a `\n` inside the
    // segment are transparent: they're the container's prefix on
    // continuation lines, not paragraph content. If the bytes after
    // a `\n` don't match `cont_prefix`, the paragraph has a shape
    // the derivation logic doesn't model and we bail out so the
    // identity path preserves its bytes.
    let relevant: Vec<&Range<usize>> = atomics
        .iter()
        .filter(|a| a.start >= seg.start && a.end <= seg.end)
        .collect();
    let mut tokens: Vec<Token<'a>> = Vec::new();
    let mut tok_start: Option<usize> = None;
    let mut i = seg.start;
    let prefix_bytes = cont_prefix.as_bytes();
    while i < seg.end {
        if let Some(a) = relevant.iter().find(|a| a.start == i) {
            if tok_start.is_none() {
                tok_start = Some(i);
            }
            i = a.end;
            continue;
        }
        let b = bytes.get(i).copied().unwrap_or(b' ');
        if b == b'\n' {
            flush_token(bytes, &mut tok_start, i, &mut tokens);
            i = i.saturating_add(1);
            // Skip the continuation prefix on the next line. If we
            // landed at the segment end, there's no continuation
            // line to prefix (the trailing `\n` is just the
            // paragraph terminator).
            if !prefix_bytes.is_empty() && i < seg.end {
                let upper = i.saturating_add(prefix_bytes.len());
                if upper > seg.end {
                    return None;
                }
                if bytes.get(i..upper) != Some(prefix_bytes) {
                    return None;
                }
                i = upper;
            }
            continue;
        }
        if b == b' ' || b == b'\t' {
            flush_token(bytes, &mut tok_start, i, &mut tokens);
            i = i.saturating_add(1);
            continue;
        }
        if tok_start.is_none() {
            tok_start = Some(i);
        }
        i = i.saturating_add(1);
    }
    flush_token(bytes, &mut tok_start, seg.end, &mut tokens);
    Some(tokens)
}

fn flush_token<'a>(bytes: &'a [u8], tok_start: &mut Option<usize>, end: usize, tokens: &mut Vec<Token<'a>>) {
    if let Some(start) = tok_start.take()
        && end > start
    {
        push_slice(bytes, start..end, tokens);
    }
}

fn push_slice<'a>(bytes: &'a [u8], range: Range<usize>, tokens: &mut Vec<Token<'a>>) {
    let Some(slice) = bytes.get(range) else {
        return;
    };
    let Ok(s) = std::str::from_utf8(slice) else {
        return;
    };
    if s.is_empty() {
        return;
    }
    let width = u32::try_from(UnicodeWidthStr::width(s)).unwrap_or(u32::MAX);
    tokens.push(Token { text: s, width });
}

fn display_width(s: &str) -> u32 {
    u32::try_from(UnicodeWidthStr::width(s)).unwrap_or(u32::MAX)
}

/// Knuth-Plass-lite: minimise total squared slack across lines.
/// Returns `None` if the time budget is exceeded or the token cap is
/// blown. Caller falls back to one-line emission.
fn layout_lines<'a>(
    tokens: &[Token<'a>],
    first_target: u32,
    cont_target: u32,
    last_line_extra: u32,
    start: Instant,
) -> Option<Vec<Vec<Token<'a>>>> {
    if tokens.is_empty() {
        return Some(Vec::new());
    }
    if tokens.len() > MAX_WRAP_TOKENS {
        tracing::warn!(
            tokens = tokens.len(),
            cap = MAX_WRAP_TOKENS,
            "wrap: paragraph exceeds token cap; emitting verbatim without re-wrap"
        );
        return None;
    }
    let n = tokens.len();
    let mut cost = vec![u64::MAX; n.saturating_add(1)];
    let mut prev = vec![0usize; n.saturating_add(1)];
    if let Some(slot) = cost.get_mut(0) {
        *slot = 0;
    }
    let mut tick: usize = 0;
    for j in 1..=n {
        for i in 0..j {
            tick = tick.saturating_add(1);
            if tick.is_multiple_of(TIME_CHECK_STRIDE) && start.elapsed() >= MAX_WRAP_TIME {
                tracing::warn!(
                    tokens = n,
                    budget_ms = MAX_WRAP_TIME.as_millis(),
                    "wrap: DP time budget exceeded; emitting verbatim without re-wrap"
                );
                return None;
            }
            let base = cost.get(i).copied().unwrap_or(u64::MAX);
            if base == u64::MAX {
                continue;
            }
            let raw_target = if i == 0 { first_target } else { cont_target };
            let target = if j == n {
                raw_target.saturating_sub(last_line_extra).max(1)
            } else {
                raw_target
            };
            let line_w = line_width(tokens, i, j);
            let bad = badness(line_w, target, j == n, j.saturating_sub(i));
            let total = base.saturating_add(bad);
            if total < cost.get(j).copied().unwrap_or(u64::MAX) {
                if let Some(slot) = cost.get_mut(j) {
                    *slot = total;
                }
                if let Some(slot) = prev.get_mut(j) {
                    *slot = i;
                }
            }
        }
    }
    let mut breaks: Vec<usize> = Vec::new();
    let mut j = n;
    while j > 0 {
        breaks.push(j);
        j = prev.get(j).copied().unwrap_or(0);
    }
    breaks.reverse();
    let mut lines: Vec<Vec<Token<'a>>> = Vec::with_capacity(breaks.len());
    let mut consumed = 0usize;
    for &end in &breaks {
        let line: Vec<Token<'a>> = tokens
            .get(consumed..end)
            .unwrap_or(&[])
            .iter()
            .map(|t| Token {
                text: t.text,
                width: t.width,
            })
            .collect();
        lines.push(line);
        consumed = end;
    }
    Some(lines)
}

fn line_width(tokens: &[Token<'_>], i: usize, j: usize) -> u32 {
    let slice = tokens.get(i..j).unwrap_or(&[]);
    let words: u32 = slice.iter().map(|t| t.width).fold(0u32, |a, b| a.saturating_add(b));
    let glues = u32::try_from(j.saturating_sub(i).saturating_sub(1)).unwrap_or(0);
    words.saturating_add(glues)
}

fn badness(line_w: u32, target: u32, is_last_line: bool, boxes_on_line: usize) -> u64 {
    if line_w <= target {
        if is_last_line {
            return 0;
        }
        let slack = u64::from(target.saturating_sub(line_w));
        slack.saturating_mul(slack)
    } else if boxes_on_line <= 1 {
        let over = u64::from(line_w.saturating_sub(target));
        OVERFLOW_PENALTY.saturating_add(over.saturating_mul(over))
    } else {
        u64::MAX
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::{FmtOptions, Wrap};

    fn wrap(input: &str, mode: Wrap) -> String {
        crate::format_document(
            &mdwright_document::Document::parse(input).expect("fixture parses"),
            &FmtOptions::default().with_wrap(mode),
        )
    }

    #[test]
    fn keep_is_noop() {
        let s = "alpha beta\ngamma delta\n";
        assert_eq!(wrap(s, Wrap::Keep), s);
    }

    #[test]
    fn no_collapses_soft_breaks() {
        let s = "alpha beta\ngamma delta\n";
        assert_eq!(wrap(s, Wrap::No), "alpha beta gamma delta\n");
    }

    #[test]
    fn at_breaks_long_paragraph() {
        let s = "alpha beta gamma delta epsilon zeta\n";
        let out = wrap(s, Wrap::At(15));
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.iter().all(|l| l.chars().count() <= 15), "{out:?}");
    }

    #[test]
    fn at_respects_atomic_code_span() {
        let s = "alpha `longish_code_token` zeta\n";
        let out = wrap(s, Wrap::At(10));
        // The code span should not be split; an oversize line is
        // acceptable.
        assert!(out.contains("`longish_code_token`"));
    }

    #[test]
    fn blockquote_continuation_keeps_prefix() {
        let s = "> alpha beta gamma delta epsilon zeta eta theta iota\n";
        let out = wrap(s, Wrap::At(20));
        // Each line starts with `> `.
        for line in out.lines() {
            if !line.is_empty() {
                assert!(line.starts_with("> "), "got {line:?}");
            }
        }
    }

    #[test]
    fn list_item_continuation_uses_indent() {
        let s = "- alpha beta gamma delta epsilon zeta eta theta\n";
        let out = wrap(s, Wrap::At(15));
        let mut iter = out.lines();
        let first = iter.next().unwrap_or("");
        assert!(first.starts_with("- "), "got {first:?}");
        for line in iter {
            if !line.is_empty() {
                assert!(line.starts_with("  "), "got {line:?}");
            }
        }
    }

    #[test]
    fn hard_break_preserves_marker() {
        let s = "first sentence.\\\nsecond sentence.\n";
        let out = wrap(s, Wrap::At(40));
        assert!(out.contains("\\\n"));
    }

    #[test]
    fn hard_break_rewrite_skips_when_list_context_would_change() {
        let s = "* \\\n|\\\n  *";
        assert_eq!(wrap(s, Wrap::No), s);
    }

    #[test]
    fn unsupported_wrap_shapes_are_reported() {
        let s = "* \\\n|\\\n  *";
        let doc = mdwright_document::Document::parse(s).expect("fixture parses");
        let (out, report) = crate::format_document_with_report(&doc, &FmtOptions::default().with_wrap(Wrap::No));
        assert_eq!(out, s);
        assert!(report.rewrite_skipped_wrap > 0, "{report:?}");
    }
}
