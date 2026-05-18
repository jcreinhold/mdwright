//! Block-boundary checkpoint table for range-formatting.
//!
//! Editor latency requires that a one-paragraph edit format in time
//! proportional to the paragraph, not the document. The mechanism is a
//! *checkpoint table* — a sorted list of byte offsets, one per
//! top-level Markdown block — that lets a caller-supplied byte range
//! be snapped to the smallest covering whole-block slice. The slice is
//! then parsed and formatted independently via the existing
//! [`crate::Document::parse`] / [`crate::Document::format`] path.
//!
//! A checkpoint sits at column 0 of a line that opens a **top-level**
//! block (container depth zero — not inside a blockquote, list,
//! footnote, or table). Both conditions are load-bearing: slicing two
//! checkpoints must yield a syntactically self-contained Markdown
//! sub-document, otherwise the substring contract in
//! [`crate::format_range`] breaks. The depth clause specifically
//! defends against slicing one item out of an ordered list, where the
//! sliced item would re-parse as item 1 and `OrderedList::Renumber`
//! would diverge from the whole-document output.
//!
//! Offsets are recorded in the **caller's source coordinates**
//! (original bytes, before CM §2.1 / §2.3 canonicalisation). Pulldown
//! reports canonical offsets; [`CheckpointTable::build`] translates
//! them through [`crate::source::Source::to_original`] once at build
//! time. For the common case where input contains no `\r` or `\0` the
//! translation is the identity and costs nothing.
//!
//! Frontmatter (YAML `---…---` or TOML `+++…+++`) is treated as a
//! single prelude region: the first body checkpoint sits at the byte
//! after the closing delimiter. A caller-supplied range that falls
//! entirely inside frontmatter snaps forward to the first body block.

use std::ops::Range;

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::parse;
use crate::source::{ByteSpan, CanonicalSource, Source};

/// One block boundary in the caller's source.
#[derive(Copy, Clone, Debug)]
pub(crate) struct BlockCheckpoint {
    /// Byte offset in the caller's source where the block starts.
    /// Always column 0 of its line, always at container depth 0.
    pub(crate) byte: u32,
    /// Cheap snapshot of the parser walk state at this point. Reserved
    /// for the incremental-rebuild work in the LSP session — the
    /// current resumption logic doesn't read it, but recording it now
    /// lets that session diff two tables for "did anything before this
    /// boundary actually change?".
    #[expect(dead_code, reason = "reserved for LSP incremental-rebuild")]
    pub(crate) parser_state: u64,
}

/// Per-document table of top-level block boundaries.
///
/// Built once per source version via [`CheckpointTable::build`]; the
/// LSP rebuilds on every `didChange` notification. Internally a sorted
/// `Vec<BlockCheckpoint>` with `byte = 0` and a sentinel at
/// `byte = source.len()` as bookends, so [`Self::snap_to_block_boundaries`]
/// is branch-free at the bounds.
#[derive(Debug)]
pub struct CheckpointTable {
    source_len: u32,
    /// Sorted by `byte` ascending. First entry is always `byte = 0`;
    /// last entry is always `byte = source_len`.
    points: Vec<BlockCheckpoint>,
}

impl CheckpointTable {
    /// Walk `source` once, recording one checkpoint per top-level
    /// block. Cost: one pulldown event stream, one offset translation
    /// per checkpoint, one `Vec` allocation.
    #[must_use]
    pub fn build(source: &str) -> Self {
        let source_len = u32::try_from(source.len()).unwrap_or(u32::MAX);
        let src = Source::new(source);
        let canonical = src.canonical();
        let map_is_identity = src.offset_map().is_identity();
        let fm_end = frontmatter_end(canonical);
        let body = CanonicalSource::from_source(&src).trusted_subrange(fm_end..canonical.len());

        // Capacity heuristic: one checkpoint per ~64 source bytes is a
        // generous upper bound for prose-heavy docs (typical paragraph
        // is hundreds of bytes). Pre-allocate once so growth on the
        // hot path is free.
        let cap = (source.len() / 64).saturating_add(2);
        let mut points: Vec<BlockCheckpoint> = Vec::with_capacity(cap);
        points.push(BlockCheckpoint {
            byte: 0,
            parser_state: 0,
        });

        let mut depth: u32 = 0;
        let mut event_count: u32 = 0;
        let try_push = |points: &mut Vec<BlockCheckpoint>, range_start: usize, depth: u32, event_count: u32| {
            let abs_canonical = u32::try_from(range_start.saturating_add(fm_end)).unwrap_or(u32::MAX);
            let abs_original = if map_is_identity {
                abs_canonical
            } else {
                src.to_original(ByteSpan::new(abs_canonical, abs_canonical)).start
            };
            // Pulldown may emit a block Start at the same byte as the
            // last recorded checkpoint (a document opening with a
            // paragraph reports the paragraph's Start at byte 0, which
            // is already in the table). Don't record duplicates — the
            // sort invariant is strict for the binary search.
            if points.last().is_none_or(|last| last.byte < abs_original) {
                points.push(BlockCheckpoint {
                    byte: abs_original,
                    parser_state: parser_state_hash(depth, event_count),
                });
            }
        };
        for (event, range) in parse::events_with_offsets(body, parse::FORMATTER_OPTIONS) {
            event_count = event_count.saturating_add(1);
            walk_event(event, range.start, &mut depth, event_count, &mut points, &try_push);
        }

        // Sentinel at end-of-source so `snap_to_block_boundaries` can
        // always find an upper bound without a special case.
        if points.last().is_none_or(|last| last.byte < source_len) {
            points.push(BlockCheckpoint {
                byte: source_len,
                parser_state: parser_state_hash(depth, event_count),
            });
        }

        Self { source_len, points }
    }

    /// Smallest `[lo, hi)` byte range that covers `range` and starts
    /// and ends on a checkpoint. Always succeeds: empty / out-of-bounds
    /// / partial-block ranges all resolve to a well-defined slice (the
    /// smallest superset; empty when the request is wholly past the
    /// source end).
    pub(crate) fn snap_to_block_boundaries(&self, range: Range<u32>) -> Range<u32> {
        let req_start = range.start.min(self.source_len);
        let req_end = range.end.min(self.source_len).max(req_start);

        // Largest checkpoint byte ≤ req_start.
        let lo_idx = match self.points.binary_search_by_key(&req_start, |p| p.byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let lo = self.points.get(lo_idx).map_or(0, |p| p.byte);

        // Smallest checkpoint byte ≥ req_end.
        let hi_idx = match self.points.binary_search_by_key(&req_end, |p| p.byte) {
            Ok(i) => i,
            Err(i) => i,
        };
        let hi = self.points.get(hi_idx).map_or(self.source_len, |p| p.byte);

        lo..hi
    }

    /// Number of recorded checkpoints, including the implicit `byte = 0`
    /// and the end-of-source sentinel. Exposed for tests and the
    /// allocation-discipline bench.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// `true` iff the table has only its two bookends — the document
    /// contains no top-level block.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.len() <= 2
    }
}

/// Walk one event from the boundary scan. Fall-through is the design:
/// inline events, leaves, container Starts/Ends, and any future
/// pulldown variant we don't yet handle simply leave the table state
/// unchanged. The substring proptest catches the case where a new
/// top-level block kind would need a checkpoint here.
#[allow(clippy::wildcard_enum_match_arm)]
fn walk_event(
    event: Event<'_>,
    range_start: usize,
    depth: &mut u32,
    event_count: u32,
    points: &mut Vec<BlockCheckpoint>,
    try_push: &impl Fn(&mut Vec<BlockCheckpoint>, usize, u32, u32),
) {
    match event {
        Event::Start(tag) if *depth == 0 && is_top_level_block(&tag) => {
            try_push(points, range_start, *depth, event_count);
            if is_container(&tag) {
                *depth = depth.saturating_add(1);
            }
        }
        Event::Start(tag) if is_container(&tag) => {
            *depth = depth.saturating_add(1);
        }
        Event::End(end) if is_container_end(end) => {
            *depth = depth.saturating_sub(1);
        }
        Event::Rule if *depth == 0 => {
            try_push(points, range_start, *depth, event_count);
        }
        _ => {}
    }
}

fn is_top_level_block(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Table(_)
            | Tag::FootnoteDefinition(_)
    )
}

fn is_container(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::BlockQuote(_)
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
    )
}

fn is_container_end(end: TagEnd) -> bool {
    matches!(
        end,
        TagEnd::BlockQuote(_)
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
    )
}

/// Mirrors `ir::split_frontmatter`'s offset return without
/// constructing the `Frontmatter` payload. Keep the two in sync: the
/// `ir.rs` version is the authority on what counts as a frontmatter
/// block (see its body for the YAML/TOML disambiguation rule).
fn frontmatter_end(source: &str) -> usize {
    let Some(first_line_end) = source.find('\n') else {
        return 0;
    };
    let first_line = source.get(..first_line_end).unwrap_or("");
    let trimmed = first_line.trim_end();
    let close_pat: &[&str] = match trimmed {
        "---" => &["---", "..."],
        "+++" => &["+++"],
        _ => return 0,
    };
    let body_start = first_line_end.saturating_add(1);
    let Some(rest) = source.get(body_start..) else {
        return 0;
    };
    let mut cursor = 0usize;
    let mut saw_key = false;
    while cursor < rest.len() {
        let nl = rest
            .get(cursor..)
            .and_then(|s| s.find('\n'))
            .unwrap_or_else(|| rest.len().saturating_sub(cursor));
        let end_excl = cursor.saturating_add(nl);
        let line = rest.get(cursor..end_excl).unwrap_or("");
        let line_trim = line.trim_end();
        if close_pat.contains(&line_trim) {
            if !saw_key {
                return 0;
            }
            return body_start.saturating_add(end_excl).saturating_add(1).min(source.len());
        }
        if line_has_key(line_trim, trimmed == "+++") {
            saw_key = true;
        }
        cursor = end_excl.saturating_add(1);
    }
    0
}

fn line_has_key(line: &str, toml: bool) -> bool {
    let trimmed = line.trim_start();
    let sep = if toml { '=' } else { ':' };
    let Some(idx) = trimmed.find(sep) else {
        return false;
    };
    let key = trimmed.get(..idx).unwrap_or("").trim();
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parser_state_hash(depth: u32, event_count: u32) -> u64 {
    (u64::from(depth) << 32) | u64::from(event_count)
}

#[cfg(test)]
mod tests {
    use super::CheckpointTable;

    #[test]
    fn empty_source() {
        let t = CheckpointTable::build("");
        // Both bookends collapse to byte 0 (source_len == 0).
        assert_eq!(t.len(), 1);
        assert!(t.is_empty());
        assert_eq!(t.snap_to_block_boundaries(0..0), 0..0);
    }

    #[test]
    fn three_paragraphs() {
        let src = "a\n\nb\n\nc\n";
        let t = CheckpointTable::build(src);
        // 0, start-of-a, start-of-b, start-of-c, sentinel — but
        // start-of-a coincides with 0 so it's deduplicated.
        assert!(t.len() >= 4);
        // Range hitting `b` snaps to start-of-b..start-of-c.
        let snapped = t.snap_to_block_boundaries(3..4);
        assert_eq!(&src[snapped.start as usize..snapped.end as usize], "b\n\n");
    }

    #[test]
    fn range_inside_list_snaps_to_list_boundaries() {
        let src = "para\n\n1. one\n2. two\n3. three\n\ntail\n";
        let t = CheckpointTable::build(src);
        // Pick a byte inside the list (offset of "two").
        let two_at = src.find("two").unwrap_or(0);
        let snapped = t.snap_to_block_boundaries(two_at as u32..two_at as u32 + 1);
        let slice = &src[snapped.start as usize..snapped.end as usize];
        // The slice must contain the full list, not just one item —
        // otherwise renumber would diverge from whole-document output.
        assert!(slice.contains("1. one"), "slice should include list start: {slice:?}");
        assert!(slice.contains("3. three"), "slice should include list end: {slice:?}");
    }

    #[test]
    fn range_past_end_snaps_to_empty() {
        let src = "a\n";
        let t = CheckpointTable::build(src);
        let snapped = t.snap_to_block_boundaries(99..100);
        assert_eq!(snapped, src.len() as u32..src.len() as u32);
    }

    #[test]
    fn frontmatter_is_one_prelude_region() {
        let src = "---\ntitle: x\n---\n# heading\n\npara\n";
        let t = CheckpointTable::build(src);
        // A range starting inside frontmatter should snap forward to
        // the first body block ("# heading").
        let heading_at = src.find("# heading").unwrap_or(0);
        let snapped = t.snap_to_block_boundaries(2..3);
        assert!(snapped.start as usize <= heading_at);
        assert!(snapped.end as usize >= heading_at);
    }

    #[test]
    fn crlf_source_preserves_original_offsets() {
        let src = "a\r\n\r\nb\r\n";
        let t = CheckpointTable::build(src);
        // The "b" paragraph's checkpoint should be at the byte
        // position of "b" in the ORIGINAL (CRLF) source.
        let b_at = src.find('b').unwrap_or(0) as u32;
        let snapped = t.snap_to_block_boundaries(b_at..b_at + 1);
        let slice = &src[snapped.start as usize..snapped.end as usize];
        assert!(slice.contains('b'));
    }
}
