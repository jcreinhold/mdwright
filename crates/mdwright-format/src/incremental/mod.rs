//! Block-boundary checkpoint table for range-formatting.
//!
//! Editor latency requires that a one-paragraph edit format in time
//! proportional to the paragraph, not the document. The mechanism is a
//! *checkpoint table* — a sorted list of byte offsets, one per
//! top-level Markdown block — that lets a caller-supplied byte range
//! be snapped to the smallest covering whole-block slice. The slice is
//! then parsed and formatted independently through the document and
//! formatter entry points.
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
//! them through `Source::to_original` once at build
//! time. For the common case where input contains no `\r` or `\0` the
//! translation is the identity and costs nothing.
//!
//! Frontmatter (YAML `---…---` or TOML `+++…+++`) is treated as a
//! single prelude region: the first body checkpoint sits at the byte
//! after the closing delimiter. A caller-supplied range that falls
//! entirely inside frontmatter snaps forward to the first body block.

use std::ops::Range;

use mdwright_document::{Document, ParseError, ParseOptions};

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
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if parser execution cannot safely
    /// recognise the source.
    pub fn build(source: &str) -> Result<Self, ParseError> {
        Self::build_with_options(source, ParseOptions::default())
    }

    /// Build a checkpoint table under explicit recognition policy.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if parser execution cannot safely
    /// recognise the source under `parse_options`.
    pub fn build_with_options(source: &str, parse_options: ParseOptions) -> Result<Self, ParseError> {
        let doc = Document::parse_with_options(source, parse_options)?;
        Ok(Self::from_document(&doc))
    }

    /// Build from an already parsed document.
    #[must_use]
    pub fn from_document(doc: &Document) -> Self {
        let facts = doc
            .block_checkpoints()
            .iter()
            .map(|point| {
                let byte = usize::try_from(point.byte).unwrap_or(usize::MAX);
                let original = doc.canonical_to_original_range(byte..byte).start;
                mdwright_document::BlockCheckpointFact {
                    byte: u32::try_from(original).unwrap_or(u32::MAX),
                    parser_state: point.parser_state,
                }
            })
            .collect();
        Self::from_facts(doc.original_source().len(), facts)
    }

    fn from_facts(source_len: usize, facts: Vec<mdwright_document::BlockCheckpointFact>) -> Self {
        let source_len = u32::try_from(source_len).unwrap_or(u32::MAX);
        let mut points: Vec<BlockCheckpoint> = facts
            .into_iter()
            .map(|point| BlockCheckpoint {
                byte: point.byte,
                parser_state: point.parser_state,
            })
            .collect();
        if points.last().is_none_or(|last| last.byte < source_len) {
            points.push(BlockCheckpoint {
                byte: source_len,
                parser_state: 0,
            });
        }

        Self { source_len, points }
    }

    /// Smallest `[lo, hi)` byte range that covers `range` and starts
    /// and ends on a checkpoint. Always succeeds: empty / out-of-bounds
    /// / partial-block ranges all resolve to a well-defined slice (the
    /// smallest superset; empty when the request is wholly past the
    /// source end).
    pub fn snap_to_block_boundaries(&self, range: Range<u32>) -> Range<u32> {
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::CheckpointTable;

    #[test]
    fn empty_source() {
        let t = CheckpointTable::build("").expect("checkpoint source parses");
        // Both bookends collapse to byte 0 (source_len == 0).
        assert_eq!(t.len(), 1);
        assert!(t.is_empty());
        assert_eq!(t.snap_to_block_boundaries(0..0), 0..0);
    }

    #[test]
    fn three_paragraphs() {
        let src = "a\n\nb\n\nc\n";
        let t = CheckpointTable::build(src).expect("checkpoint source parses");
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
        let t = CheckpointTable::build(src).expect("checkpoint source parses");
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
        let t = CheckpointTable::build(src).expect("checkpoint source parses");
        let snapped = t.snap_to_block_boundaries(99..100);
        assert_eq!(snapped, src.len() as u32..src.len() as u32);
    }

    #[test]
    fn frontmatter_is_one_prelude_region() {
        let src = "---\ntitle: x\n---\n# heading\n\npara\n";
        let t = CheckpointTable::build(src).expect("checkpoint source parses");
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
        let t = CheckpointTable::build(src).expect("checkpoint source parses");
        // The "b" paragraph's checkpoint should be at the byte
        // position of "b" in the ORIGINAL (CRLF) source.
        let b_at = src.find('b').unwrap_or(0) as u32;
        let snapped = t.snap_to_block_boundaries(b_at..b_at + 1);
        let slice = &src[snapped.start as usize..snapped.end as usize];
        assert!(slice.contains('b'));
    }
}
