//! Paragraph wrap as a post-pass over rendered output bytes.
//!
//! Knuth-Plass-lite squared-slack DP targeting byte ranges.
//!
//! ## Contract
//!
//! [`wrap_paragraphs`] walks `out`'s pulldown event stream to find
//! every `Tag::Paragraph` and rewrites the paragraph's bytes per
//! [`Wrap`]:
//!
//! - `Wrap::Keep` — no-op (identity emit already preserved breaks).
//! - `Wrap::No` — collapse every soft break inside a paragraph to a
//!   single space; hard breaks (CM `\` or two-trailing-space) keep
//!   the line boundary.
//! - `Wrap::At(n)` — reflow each paragraph so no line exceeds `n`
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
//! For each paragraph, the rewrite extracts inline atomics by source
//! byte range, tokenises the remaining text on whitespace, applies
//! the DP, and re-emits with `\n` + the continuation prefix between
//! lines. Because the source bytes of inline atomics are copied
//! verbatim, the pulldown event stream over the rewritten paragraph
//! agrees with the original modulo soft-break positions —
//! semantically equivalent under
//! [`crate::format::semantic::semantically_equivalent`] by
//! construction.

use std::ops::Range;
use std::time::{Duration, Instant};

use pulldown_cmark::{Event, Tag, TagEnd};
use unicode_width::UnicodeWidthStr;

use crate::config::Wrap;
use crate::format::semantic::semantically_equivalent;
use crate::parse::{self, FORMATTER_OPTIONS};
use crate::source::{CanonicalSource, Source};

/// Time budget per paragraph for the DP.
const MAX_WRAP_TIME: Duration = Duration::from_millis(100);
const TIME_CHECK_STRIDE: usize = 1 << 10;
const MAX_WRAP_TOKENS: usize = 100_000;
const OVERFLOW_PENALTY: u64 = 1_000_000;

/// Apply the wrap policy to every paragraph in `out`. No-op when
/// `mode` is [`Wrap::Keep`].
pub(crate) fn wrap_paragraphs(out: &mut String, mode: Wrap) {
    if matches!(mode, Wrap::Keep) {
        return;
    }
    let paragraphs = collect_paragraphs(out);
    for p in paragraphs.into_iter().rev() {
        let Some(replacement) = rewrap_paragraph(out, &p, mode) else {
            continue;
        };
        let Some(existing) = out.get(p.line_lo..p.line_hi) else {
            continue;
        };
        if replacement == existing {
            continue;
        }
        // Verify the rewrite preserves the parse. The wrap pass
        // changes whitespace inside a paragraph; in pathological
        // cases (e.g. a paragraph whose collapsed words spell out a
        // thematic break `_ _ _`) the reparse diverges. Skip the
        // rewrite when that happens so the source bytes survive.
        if !semantically_equivalent(existing, &replacement) {
            continue;
        }
        out.replace_range(p.line_lo..p.line_hi, &replacement);
    }
}

/// Everything the rewrite needs about one paragraph instance.
struct Paragraph {
    /// Byte range covering the paragraph's physical lines: from the
    /// start of the first line (column 0) through the trailing `\n`
    /// of the last line. This is the slice we replace.
    line_lo: usize,
    line_hi: usize,
    /// Byte range pulldown reported as the paragraph content.
    content_lo: usize,
    content_hi: usize,
    /// Container prefix copied from the first line's `[line_lo, content_lo)`
    /// bytes verbatim.
    first_prefix: String,
    /// Continuation-line prefix derived from `first_prefix` by
    /// replacing list markers with same-width spaces.
    cont_prefix: String,
    /// Inline atomic source ranges (absolute byte offsets) inside the
    /// paragraph: code spans, link/image source-byte spans, raw inline
    /// HTML, inline math, display math. These never split across a
    /// line boundary.
    atomics: Vec<Range<usize>>,
    /// Absolute byte offsets of `\n` characters preceded by an
    /// in-source hard-break marker (`\` or `  `). The rewrite re-emits
    /// the hard-break marker verbatim before forcing a line boundary.
    hard_breaks: Vec<HardBreak>,
}

#[derive(Clone, Copy)]
struct HardBreak {
    /// Byte position of the marker (the `\` byte for `\\\n` style, or
    /// the first space byte for the `  \n` style).
    marker_lo: usize,
    /// Byte position of the terminating `\n`.
    nl: usize,
    /// Literal marker bytes ("\\" or "  ").
    marker: &'static str,
}

fn collect_paragraphs(out: &str) -> Vec<Paragraph> {
    let src = Source::new(out);
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let bytes = out.as_bytes();

    // Per-event state machine: when we open a Paragraph we start
    // collecting atomics + hard breaks until the matching End. Tight
    // list items don't emit a Tag::Paragraph wrapper around their
    // content; we synthesise one by opening on Tag::Item and closing
    // at the matching End / nested-block boundary so list-item prose
    // also wraps.
    let mut current: Option<PartialParagraph> = None;
    let mut paragraph_depth: u32 = 0;
    let mut item_depth: u32 = 0;

    for (ev, range) in parse::events_with_offsets(CanonicalSource::from_source(&src), FORMATTER_OPTIONS) {
        match ev {
            Event::Start(Tag::Paragraph) => {
                // Replace any synthetic Item region with the real
                // paragraph — pulldown emits the inline content
                // wrapped in Paragraph for loose lists.
                if paragraph_depth == 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                paragraph_depth = paragraph_depth.saturating_add(1);
            }
            Event::End(TagEnd::Paragraph) => {
                paragraph_depth = paragraph_depth.saturating_sub(1);
                if paragraph_depth == 0
                    && let Some(p) = current.take()
                    && let Some(finished) = p.finish(bytes)
                {
                    paragraphs.push(finished);
                }
            }
            Event::Start(Tag::Item) => {
                item_depth = item_depth.saturating_add(1);
            }
            Event::End(TagEnd::Item) => {
                item_depth = item_depth.saturating_sub(1);
                if let Some(p) = current.take()
                    && let Some(finished) = p.finish(bytes)
                {
                    paragraphs.push(finished);
                }
            }
            Event::Start(
                Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::List(_)
                | Tag::Table(_)
                | Tag::FootnoteDefinition(_)
                | Tag::DefinitionList
                | Tag::MetadataBlock(_),
            ) => {
                // A block-level child inside an Item closes any
                // synthetic paragraph we'd opened for inline content.
                if let Some(p) = current.take()
                    && let Some(finished) = p.finish(bytes)
                {
                    paragraphs.push(finished);
                }
            }
            Event::Text(_) => {
                if current.is_none() && paragraph_depth == 0 && item_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::Code(_) | Event::InlineHtml(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                if current.is_none() && paragraph_depth == 0 && item_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut() {
                    p.atomics.push(range.clone());
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            Event::SoftBreak => {
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                if current.is_none() && paragraph_depth == 0 && item_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut() {
                    p.link_stack.push(range.start);
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                if let Some(p) = current.as_mut() {
                    if let Some(start) = p.link_stack.pop() {
                        p.atomics.push(start..range.end);
                    }
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            // Inline wrappers (emphasis / strong / strikethrough /
            // sub-/super-script) don't carry text themselves but their
            // delimiter bytes are part of the paragraph content. Open
            // the synthetic paragraph on their start so the leading
            // delimiter is captured, and extend `content_hi` on their
            // end so the trailing delimiter is included too.
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Superscript | Tag::Subscript) => {
                if current.is_none() && paragraph_depth == 0 && item_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::End(
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Superscript | TagEnd::Subscript,
            ) => {
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::HardBreak => {
                if let Some(p) = current.as_mut() {
                    if let Some(hb) = classify_hard_break(bytes, range.start, range.end) {
                        p.hard_breaks.push(hb);
                    }
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            Event::Start(_)
            | Event::End(_)
            | Event::Html(_)
            | Event::FootnoteReference(_)
            | Event::Rule
            | Event::TaskListMarker(_) => {}
        }
    }
    paragraphs
}

struct PartialParagraph {
    content_lo: usize,
    content_hi: usize,
    atomics: Vec<Range<usize>>,
    hard_breaks: Vec<HardBreak>,
    link_stack: Vec<usize>,
}

impl PartialParagraph {
    fn new(range: Range<usize>) -> Self {
        Self {
            content_lo: range.start,
            content_hi: range.end,
            atomics: Vec::new(),
            hard_breaks: Vec::new(),
            link_stack: Vec::new(),
        }
    }

    fn finish(self, bytes: &[u8]) -> Option<Paragraph> {
        let (line_lo, first_prefix) = extract_first_prefix(bytes, self.content_lo)?;
        let line_hi = extract_line_hi(bytes, self.content_hi);
        let cont_prefix = derive_continuation_prefix(&first_prefix)?;
        let mut atomics = self.atomics;
        atomics.sort_by_key(|r| r.start);
        let mut hard_breaks = self.hard_breaks;
        hard_breaks.sort_by_key(|h| h.nl);
        Some(Paragraph {
            line_lo,
            line_hi,
            content_lo: self.content_lo,
            content_hi: self.content_hi,
            first_prefix,
            cont_prefix,
            atomics,
            hard_breaks,
        })
    }
}

fn classify_hard_break(bytes: &[u8], start: usize, end: usize) -> Option<HardBreak> {
    // The HardBreak event range spans the marker through the
    // terminating `\n`. Inspect the bytes to decide which style:
    // `\\\n` (backslash + newline) or `  \n` (two trailing spaces +
    // newline). Other shapes (longer space runs) collapse to the
    // two-space form on re-emit.
    let slice = bytes.get(start..end)?;
    let nl_off = slice.iter().rposition(|&b| b == b'\n')?;
    let nl = start.saturating_add(nl_off);
    let before_nl = bytes.get(nl.checked_sub(1)?).copied()?;
    if before_nl == b'\\' {
        // Confirm not an escaped backslash (`\\\\` is two literal
        // backslashes, not a hard break).
        let two_back = nl.checked_sub(2).and_then(|i| bytes.get(i).copied());
        if matches!(two_back, Some(b'\\')) {
            return None;
        }
        return Some(HardBreak {
            marker_lo: nl.saturating_sub(1),
            nl,
            marker: "\\",
        });
    }
    if before_nl == b' ' {
        let two_back = nl.checked_sub(2).and_then(|i| bytes.get(i).copied());
        if matches!(two_back, Some(b' ')) {
            return Some(HardBreak {
                marker_lo: nl.saturating_sub(2),
                nl,
                marker: "  ",
            });
        }
    }
    None
}

fn extract_first_prefix(bytes: &[u8], content_lo: usize) -> Option<(usize, String)> {
    let line_lo = bytes
        .get(..content_lo)?
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |p| p.saturating_add(1));
    let prefix = bytes.get(line_lo..content_lo)?;
    let s = std::str::from_utf8(prefix).ok()?.to_owned();
    Some((line_lo, s))
}

fn extract_line_hi(bytes: &[u8], content_hi: usize) -> usize {
    let len = bytes.len();
    let content_hi = content_hi.min(len);
    // pulldown's End(Paragraph) event range end is sometimes just past
    // the paragraph's trailing newline (so `bytes[content_hi - 1]` is
    // `\n`), sometimes just past the last non-whitespace char of the
    // last line (so we have to scan forward for the terminator). The
    // two cases collapse to: "the boundary is the byte after the
    // paragraph's final `\n`, or end-of-file if absent."
    if content_hi > 0 && bytes.get(content_hi.saturating_sub(1)).copied() == Some(b'\n') {
        return content_hi;
    }
    let Some(tail) = bytes.get(content_hi..) else {
        return len;
    };
    tail.iter()
        .position(|&b| b == b'\n')
        .map_or(len, |p| content_hi.saturating_add(p).saturating_add(1))
}

/// Derive the continuation-line prefix from the first-line prefix.
/// Blockquote `>` markers are preserved (continuation lines need to
/// stay inside the same blockquote); list markers (`-`, `*`, `+`,
/// `1.`, `1)`) are replaced with same-width spaces so continuation
/// lines align under the marker.
fn derive_continuation_prefix(first: &str) -> Option<String> {
    let bytes = first.as_bytes();
    let mut out = String::with_capacity(first.len());
    let mut i = 0usize;
    while let Some(b) = bytes.get(i).copied() {
        match b {
            b'>' => {
                out.push('>');
                i = i.saturating_add(1);
                if bytes.get(i).copied() == Some(b' ') {
                    out.push(' ');
                    i = i.saturating_add(1);
                }
            }
            b' ' | b'\t' => {
                out.push(b as char);
                i = i.saturating_add(1);
            }
            b'-' | b'*' | b'+' => {
                out.push(' ');
                i = i.saturating_add(1);
                if bytes.get(i).copied() == Some(b' ') {
                    out.push(' ');
                    i = i.saturating_add(1);
                }
            }
            b'0'..=b'9' => {
                let start = i;
                while bytes.get(i).copied().is_some_and(|c| c.is_ascii_digit()) {
                    i = i.saturating_add(1);
                }
                if matches!(bytes.get(i).copied(), Some(b'.' | b')')) {
                    i = i.saturating_add(1);
                }
                if bytes.get(i).copied() == Some(b' ') {
                    i = i.saturating_add(1);
                }
                let consumed = i.saturating_sub(start);
                for _ in 0..consumed {
                    out.push(' ');
                }
            }
            _ => {
                // Unknown prefix byte (footnote `[^id]:`, definition
                // list `:`, etc.) — return `None` so the caller skips
                // wrapping this paragraph and the identity pass keeps
                // its bytes intact.
                return None;
            }
        }
    }
    Some(out)
}

/// Re-emit a paragraph per `mode`. Returns `None` if the rewrite
/// would either be a no-op or run into a degenerate edge case (empty
/// paragraph, oversize token cap, time budget exceeded).
fn rewrap_paragraph(out: &str, p: &Paragraph, mode: Wrap) -> Option<String> {
    let bytes = out.as_bytes();
    let content = bytes.get(p.content_lo..p.content_hi)?;
    let source_had_trailing_nl = p.line_hi > 0 && bytes.get(p.line_hi.saturating_sub(1)).copied() == Some(b'\n');
    let segments = split_at_hard_breaks(p, bytes);
    let first_prefix_width = display_width(&p.first_prefix);
    let cont_prefix_width = display_width(&p.cont_prefix);
    let target = match mode {
        Wrap::Keep => return None,
        Wrap::No => u32::MAX,
        Wrap::At(n) => n.max(1),
    };

    let mut emitted = String::with_capacity(content.len().saturating_add(p.first_prefix.len()));
    emitted.push_str(&p.first_prefix);

    let start = Instant::now();
    for (seg_idx, seg) in segments.iter().enumerate() {
        let tokens = tokenize_segment(bytes, seg, &p.atomics, &p.cont_prefix)?;
        let first_target = if seg_idx == 0 {
            target.saturating_sub(first_prefix_width).max(1)
        } else {
            target.saturating_sub(cont_prefix_width).max(1)
        };
        let cont_target = target.saturating_sub(cont_prefix_width).max(1);
        // Non-terminal segments end with a hard-break marker (`\` or
        // `  `) appended to the last line — the marker has to fit
        // inside the wrap budget too. Shrink the per-segment last-
        // line target by the marker's display width.
        let last_line_extra = if seg_idx.saturating_add(1) < segments.len() {
            p.hard_breaks.get(seg_idx).map_or(0, |h| display_width(h.marker))
        } else {
            0
        };
        let lines = if tokens.is_empty() {
            Vec::new()
        } else {
            match mode {
                Wrap::No => vec![tokens],
                Wrap::At(_) => layout_lines(&tokens, first_target, cont_target, last_line_extra, start)?,
                Wrap::Keep => return None,
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
                emitted.push_str(&p.cont_prefix);
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
            let hb = p.hard_breaks.get(seg_idx)?;
            emitted.push_str(hb.marker);
            emitted.push('\n');
            emitted.push_str(&p.cont_prefix);
        }
    }
    if source_had_trailing_nl {
        emitted.push('\n');
    }
    Some(emitted)
}

/// A wrap-token: either a verbatim slice of source bytes (atomic or
/// word).
struct Token<'a> {
    text: &'a str,
    width: u32,
}

fn split_at_hard_breaks(p: &Paragraph, bytes: &[u8]) -> Vec<Range<usize>> {
    let mut cuts: Vec<usize> = Vec::with_capacity(p.hard_breaks.len().saturating_add(1));
    let mut start = p.content_lo;
    for hb in &p.hard_breaks {
        if hb.marker_lo >= start && hb.nl < p.content_hi {
            cuts.push(start);
            start = hb.nl.saturating_add(1);
        }
    }
    cuts.push(start);
    // Build [start..end) segments from cuts.
    let mut segments: Vec<Range<usize>> = Vec::with_capacity(cuts.len());
    for (i, &lo) in cuts.iter().enumerate() {
        let hi = if i.saturating_add(1) < cuts.len() {
            // End of segment is the marker_lo of the next hard break.
            p.hard_breaks.get(i).map_or(p.content_hi, |h| h.marker_lo)
        } else {
            p.content_hi
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
        segments.push(p.content_lo..p.content_hi);
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
    // segment are transparent — they're the container's prefix on
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
/// blown — caller falls back to one-line emission.
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
mod tests {
    use super::*;
    use crate::config::Wrap;

    fn wrap(input: &str, mode: Wrap) -> String {
        let mut out = input.to_owned();
        wrap_paragraphs(&mut out, mode);
        out
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
}
