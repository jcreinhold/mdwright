//! Owns the bytes a document was parsed from, in two forms.
//!
//! `Source` is the single module that knows the difference between
//! the **original** bytes (what the caller passed) and the
//! **canonical** bytes pulldown-cmark parses against (CM §2.1 line
//! endings + CM §2.3 NUL → U+FFFD, both applied once at construction).
//! Every other module either operates on canonical coordinates (the
//! IR, the formatter) or asks `Source` to translate back to original
//! coordinates (diagnostics, `apply_safe_fixes`).
//!
//! Why two buffers exist: pulldown's emphasis-flanking rule treats
//! NUL and U+FFFD differently (NUL is one byte; FFFD is three),
//! so byte sequences that span an emphasis-context boundary parse
//! differently before and after canonicalisation. Owning the
//! canonical buffer here, once, means the formatter's output and the
//! source it round-trips against agree on byte classes. Owning the
//! original buffer here, too, means user-facing diagnostics and
//! `apply_safe_fixes` edits land at the byte positions the caller's
//! file actually has — CRLF endings, original NULs, and every other
//! byte the canonical pass would mask are preserved verbatim in the
//! output for spans the formatter didn't touch.
//!
//! `ByteSpan` and `OriginalSpan` are deliberately not interchangeable.
//! There is no `From<ByteSpan> for OriginalSpan`; translation goes
//! through `Source::to_original`. The type system is the discipline
//! that prevents the same drift the refactor was designed to eliminate.

use std::ops::Range;

use crate::line_index::LineIndex;

/// U+FFFD as UTF-8 bytes.
const REPLACEMENT_UTF8: &str = "\u{FFFD}";

/// A byte span in [`Source::canonical`]. Every IR type stores spans
/// of this kind. Use [`Source::text`] to materialise the bytes.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ByteSpan {
    pub start: u32,
    pub end: u32,
}

impl ByteSpan {
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "ByteSpan start > end");
        Self { start, end }
    }

    /// Construct from a `Range<usize>` produced by pulldown's
    /// `OffsetIter` against `Source::canonical`. Panics in debug if
    /// either bound exceeds `u32::MAX`; pulldown's bounds are
    /// canonical-byte offsets and Markdown documents past 4 GiB are
    /// out of scope for the library.
    #[must_use]
    pub fn from_range(r: Range<usize>) -> Self {
        debug_assert!(u32::try_from(r.end).is_ok(), "ByteSpan offset overflows u32");
        Self {
            start: r.start as u32,
            end: r.end as u32,
        }
    }

    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    #[must_use]
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// A byte span in [`Source::original`]. Used by `Diagnostic`, `Fix`,
/// and `Document::apply_safe_fixes` so user-facing output references
/// the file the caller passed. Translation from [`ByteSpan`] is the
/// one boundary; see [`Source::to_original`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct OriginalSpan {
    pub start: u32,
    pub end: u32,
}

impl OriginalSpan {
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "OriginalSpan start > end");
        Self { start, end }
    }

    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    #[must_use]
    pub fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }
}

/// Sorted log of canonicalisation rewrites, each recording the
/// canonical-byte range produced and the original-byte range
/// consumed. Between rewrites, mapping is identity-with-shift.
///
/// Identity is the common case (modern LF UTF-8 with no NUL) and is
/// represented by an empty `events` vector — `to_original` returns
/// the input span unchanged in O(1).
#[derive(Clone, Debug, Default)]
pub struct OffsetMap {
    /// Sorted by `canonical.start`. Empty ⇒ identity map.
    events: Vec<Rewrite>,
}

/// One canonicalisation event: `canonical` is the byte range emitted
/// into [`Source::canonical`], `original` is the byte range consumed
/// from [`Source::original`]. Lengths differ in general:
///
/// - `\r\n` → `\n`: `canonical.len() == 1`, `original.len() == 2`.
/// - bare `\r` → `\n`: `canonical.len() == 1`, `original.len() == 1`.
/// - `\0` → U+FFFD: `canonical.len() == 3`, `original.len() == 1`.
#[derive(Copy, Clone, Debug)]
struct Rewrite {
    canonical: ByteSpan,
    original: OriginalSpan,
}

impl OffsetMap {
    #[must_use]
    pub fn identity() -> Self {
        Self { events: Vec::new() }
    }

    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.events.is_empty()
    }

    /// Translate a canonical-byte position to the corresponding
    /// original-byte position, rounding **down** if `p` falls inside
    /// a rewrite event's canonical range (so the resulting position
    /// sits at the start of the original event).
    fn start_to_original(&self, p: u32) -> u32 {
        if self.events.is_empty() {
            return p;
        }
        // Find the last event whose canonical.start <= p.
        let idx = match self.events.binary_search_by_key(&p, |e| e.canonical.start) {
            Ok(i) => i,
            Err(0) => return p, // p sits before any rewrite — identity
            Err(i) => i.saturating_sub(1),
        };
        let Some(e) = self.events.get(idx).copied() else {
            return p;
        };
        if p < e.canonical.end {
            // p sits inside the rewrite's canonical range — round
            // down to the original event's start.
            e.original.start
        } else {
            // p sits in the identity-run after this rewrite.
            e.original.end.saturating_add(p.saturating_sub(e.canonical.end))
        }
    }

    /// Translate a canonical-byte end-bound to the corresponding
    /// original-byte end-bound, rounding **up** if `p` falls strictly
    /// inside a rewrite event's canonical range (so the bound moves
    /// past the original rewritten characters).
    fn end_to_original(&self, p: u32) -> u32 {
        if self.events.is_empty() {
            return p;
        }
        // Find the last event whose canonical.start < p (so p is at
        // or past its start). Searching by canonical.start with the
        // same protocol as start_to_original keeps the two paths
        // consistent.
        let idx = match self.events.binary_search_by_key(&p, |e| e.canonical.start) {
            // p == e.canonical.start: the bound sits at the rewrite's
            // canonical start — bound encloses zero rewritten bytes,
            // so the original bound also sits at e.original.start.
            Ok(i) => return self.events.get(i).map_or(p, |e| e.original.start),
            Err(0) => return p, // p sits before any rewrite — identity
            Err(i) => i.saturating_sub(1),
        };
        let Some(e) = self.events.get(idx).copied() else {
            return p;
        };
        if p <= e.canonical.end {
            // p is inside or right at the end of the rewrite — round
            // up to include the entire original event.
            e.original.end
        } else {
            // p sits in the identity-run after this rewrite.
            e.original.end.saturating_add(p.saturating_sub(e.canonical.end))
        }
    }
}

/// Owns the caller-supplied original bytes and the canonical view
/// pulldown parses against, plus the offset map to translate between
/// them and a line index over the original bytes.
#[derive(Debug)]
pub struct Source {
    original: String,
    canonical: String,
    map: OffsetMap,
    line_index: LineIndex,
}

impl Source {
    /// Canonicalise `raw` for parsing. Performs one linear scan that
    /// detects whether any canonicalisation is needed and short-
    /// circuits to `canonical == original` (with an identity offset
    /// map) when not.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let (canonical, map) = canonicalise(raw);
        let original = raw.to_owned();
        let line_index = LineIndex::new(&original);
        Self {
            original,
            canonical,
            map,
            line_index,
        }
    }

    /// The caller's bytes, byte-for-byte.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    /// The canonical bytes pulldown sees. Equal to [`Self::original`]
    /// when no canonicalisation was needed.
    #[must_use]
    pub fn canonical(&self) -> &str {
        &self.canonical
    }

    /// Slice the canonical buffer.
    ///
    /// # Panics
    ///
    /// Panics if `span` is not on a UTF-8 boundary or extends past
    /// the canonical buffer. Spans produced from pulldown event
    /// ranges satisfy both conditions.
    #[must_use]
    pub fn text(&self, span: ByteSpan) -> &str {
        &self.canonical[span.range()]
    }

    /// Slice the original buffer at user-facing coordinates.
    ///
    /// # Panics
    ///
    /// Panics if `span` is not on a UTF-8 boundary or extends past
    /// the original buffer.
    #[must_use]
    pub fn original_text(&self, span: OriginalSpan) -> &str {
        &self.original[span.range()]
    }

    /// Translate a canonical-byte span to the corresponding original
    /// span. The output is rounded outward — start floored, end
    /// ceilinged across change points — so the original span always
    /// covers at least the bytes the canonical span covered. This
    /// matters at two boundaries:
    ///
    /// - A canonical span ending mid-`\r\n` should include the `\r`
    ///   the canonicalisation dropped.
    /// - A canonical span starting or ending inside a U+FFFD's three
    ///   bytes should map to the single-byte original `\0`.
    #[must_use]
    pub fn to_original(&self, span: ByteSpan) -> OriginalSpan {
        if self.map.is_identity() {
            return OriginalSpan {
                start: span.start,
                end: span.end,
            };
        }
        OriginalSpan {
            start: self.map.start_to_original(span.start),
            end: self.map.end_to_original(span.end),
        }
    }

    /// Line index over the original bytes. User-facing diagnostics
    /// use this so `line:col` matches the file on disk.
    #[must_use]
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    /// The offset map. Exposed for tests and instrumentation; most
    /// callers should use [`Self::to_original`] instead.
    #[must_use]
    pub fn offset_map(&self) -> &OffsetMap {
        &self.map
    }
}

/// One linear scan of `raw` that (1) detects whether canonicalisation
/// is a no-op, returning an owned clone with an identity map, or
/// (2) produces the canonical buffer and a change-point map.
fn canonicalise(raw: &str) -> (String, OffsetMap) {
    let bytes = raw.as_bytes();

    // Fast-path: walk once, OR-folding the bytes we'd rewrite. If
    // none are present, the canonical buffer equals the input.
    let mut needs_rewrite = false;
    for &b in bytes {
        if b == b'\r' || b == b'\0' {
            needs_rewrite = true;
            break;
        }
    }
    if !needs_rewrite {
        return (raw.to_owned(), OffsetMap::identity());
    }

    // Slow path: rewrite. Capacity heuristic: reserve input length;
    // expansion-heavy input grows the buffer cheaply.
    let mut canonical = String::with_capacity(raw.len());
    let mut events: Vec<Rewrite> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let Some(&b) = bytes.get(i) else { break };
        if b == b'\r' {
            let orig_start = i as u32;
            let canon_start = canonical.len() as u32;
            canonical.push('\n');
            let consumed_cr = if bytes.get(i.saturating_add(1)).copied() == Some(b'\n') {
                2
            } else {
                1
            };
            i = i.saturating_add(consumed_cr);
            events.push(Rewrite {
                canonical: ByteSpan {
                    start: canon_start,
                    end: canonical.len() as u32,
                },
                original: OriginalSpan {
                    start: orig_start,
                    end: orig_start.saturating_add(consumed_cr as u32),
                },
            });
        } else if b == b'\0' {
            let orig_start = i as u32;
            let canon_start = canonical.len() as u32;
            canonical.push_str(REPLACEMENT_UTF8);
            i = i.saturating_add(1);
            events.push(Rewrite {
                canonical: ByteSpan {
                    start: canon_start,
                    end: canonical.len() as u32,
                },
                original: OriginalSpan {
                    start: orig_start,
                    end: orig_start.saturating_add(1),
                },
            });
        } else {
            // Copy this UTF-8 codepoint verbatim. Stepping byte-by-
            // byte is safe — pushing each byte as char would mangle
            // non-ASCII; instead, locate the codepoint boundary and
            // push the &str slice.
            let cp_end = utf8_codepoint_end(bytes, i);
            if let Some(slice) = raw.get(i..cp_end) {
                canonical.push_str(slice);
            }
            i = cp_end;
        }
    }

    (canonical, OffsetMap { events })
}

/// Length of the UTF-8 codepoint starting at `bytes[i]`. Assumes
/// well-formed UTF-8 — `raw` arrived as `&str`, so this holds.
fn utf8_codepoint_end(bytes: &[u8], i: usize) -> usize {
    let Some(&b) = bytes.get(i) else {
        return i;
    };
    let len = if b < 0x80 {
        1
    } else if b < 0xC0 {
        // continuation byte: shouldn't happen at a codepoint start
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    };
    i.saturating_add(len).min(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(s: u32, e: u32) -> ByteSpan {
        ByteSpan::new(s, e)
    }

    #[test]
    fn lf_only_input_uses_identity_map() {
        let src = Source::new("hello\nworld\n");
        assert!(src.offset_map().is_identity());
        assert_eq!(src.canonical(), "hello\nworld\n");
        assert_eq!(src.original(), src.canonical());
    }

    #[test]
    fn crlf_collapses_and_map_shifts_positively() {
        let src = Source::new("a\r\nb\r\nc\n");
        assert_eq!(src.canonical(), "a\nb\nc\n");
        // `b` is canonical byte 2 → original byte 3
        let span_b = span(2, 3);
        let orig = src.to_original(span_b);
        assert_eq!(src.original_text(orig), "b");
    }

    #[test]
    fn bare_cr_collapses() {
        let src = Source::new("a\rb\rc");
        assert_eq!(src.canonical(), "a\nb\nc");
        // Identity check: identity map iff no rewrites.
        assert!(!src.offset_map().is_identity());
    }

    #[test]
    fn nul_expands_to_ffd_and_map_shifts_negatively() {
        let src = Source::new("a\0b");
        assert_eq!(src.canonical(), "a\u{FFFD}b");
        // `b` is canonical byte 4 → original byte 2
        let span_b = span(4, 5);
        let orig = src.to_original(span_b);
        assert_eq!(src.original_text(orig), "b");
    }

    #[test]
    fn span_straddling_ffd_rounds_outward() {
        let src = Source::new("a\0b");
        // Canonical: 'a'(0..1) FFFD(1..4) 'b'(4..5). Span covering
        // just the FFFD maps to the original NUL.
        let orig = src.to_original(span(1, 4));
        assert_eq!(src.original_text(orig), "\0");
    }

    #[test]
    fn span_inside_ffd_rounds_outward_both_ends() {
        let src = Source::new("a\0b");
        // A span landing in the middle of the FFFD's three bytes
        // should still cover the original NUL byte-for-byte.
        let orig = src.to_original(span(2, 3));
        assert_eq!(src.original_text(orig), "\0");
    }

    #[test]
    fn span_inside_crlf_rounds_to_include_cr() {
        let src = Source::new("a\r\nb");
        // Canonical: 'a'(0..1) '\n'(1..2) 'b'(2..3). A span covering
        // just the canonical '\n' should map to the original '\r\n'.
        let orig = src.to_original(span(1, 2));
        assert_eq!(src.original_text(orig), "\r\n");
    }

    #[test]
    fn span_across_event_uses_correct_shift() {
        let src = Source::new("a\r\nb\0c");
        // Canonical: 'a'(0..1) '\n'(1..2) 'b'(2..3) FFFD(3..6) 'c'(6..7).
        // Original: 'a'(0..1) '\r\n'(1..3) 'b'(3..4) '\0'(4..5) 'c'(5..6).
        // Span covering 'b' (canon 2..3) → original 'b' (3..4).
        let orig = src.to_original(span(2, 3));
        assert_eq!(src.original_text(orig), "b");
        // Span covering 'c' (canon 6..7) → original 'c' (5..6).
        let orig = src.to_original(span(6, 7));
        assert_eq!(src.original_text(orig), "c");
    }

    #[test]
    fn span_with_non_ascii_codepoint_preserved() {
        // Confirm canonicalisation doesn't mangle multi-byte UTF-8
        // characters when it has to take the slow path (NUL forces
        // the rewrite).
        let src = Source::new("α\0β");
        assert_eq!(src.canonical(), "α\u{FFFD}β");
        let orig = src.to_original(span(2, 5));
        assert_eq!(src.original_text(orig), "\0");
    }

    #[test]
    fn mixed_canonicalisation_roundtrip() {
        let src = Source::new("x\r\n\0y\r\n");
        // Canonical: "x\n\u{FFFD}y\n" — bytes: 'x'(1) '\n'(1)
        // FFFD(3) 'y'(1) '\n'(1) = 7 bytes
        assert_eq!(src.canonical(), "x\n\u{FFFD}y\n");
        // Canonical 'y' is at byte 5; original 'y' at byte 4.
        let s = span(5, 6);
        let orig = src.to_original(s);
        assert_eq!(src.original_text(orig), "y");
    }

    #[test]
    fn empty_input() {
        let src = Source::new("");
        assert!(src.offset_map().is_identity());
        assert_eq!(src.canonical(), "");
    }

    #[test]
    fn identity_to_original_is_zero_cost() {
        let src = Source::new("plain text\n");
        let s = span(0, 11);
        let o = src.to_original(s);
        assert_eq!((o.start, o.end), (0, 11));
    }
}
