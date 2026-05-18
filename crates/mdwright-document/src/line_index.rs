//! Byte-offset → (line, column) mapping over a Markdown source string.
//!
//! pulldown-cmark hands us byte ranges; lint diagnostics report
//! `file:line:col` in the editor convention (both 1-indexed, columns
//! counted in Unicode codepoints, not bytes). This helper records the
//! line-start table once and answers every offset lookup in O(log n).
//!
//! The index owns no borrow on the source; callers pass the source
//! `&str` at query time. This lets [`crate::source::Source`] keep
//! ownership of the bytes without forcing a lifetime parameter on
//! every type that holds a `LineIndex`. The intended pattern is one
//! `LineIndex` built from `Source::original` at parse time, shared
//! by reference for the document's lifetime.

use std::fmt;

/// Maps byte offsets in a source string to 1-indexed (line, column)
/// pairs. Columns count Unicode codepoints, matching what `grep -n`
/// and most editors display.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// `line_starts[i]` is the byte offset of the first byte of line
    /// `i + 1`. `line_starts[0]` is always `0`. The table has one
    /// entry per newline plus one for the document start, so a final
    /// non-terminated line still has a start entry.
    line_starts: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineIndexError {
    OffsetOutOfBounds { byte: usize, source_len: usize },
    OffsetTooLarge { byte: usize },
    NotUtf8Boundary { byte: usize },
}

impl fmt::Display for LineIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OffsetOutOfBounds { byte, source_len } => {
                write!(f, "byte offset {byte} past source length {source_len}")
            }
            Self::OffsetTooLarge { byte } => write!(f, "byte offset {byte} exceeds u32 range"),
            Self::NotUtf8Boundary { byte } => write!(f, "byte {byte} not on a UTF-8 boundary"),
        }
    }
}

impl std::error::Error for LineIndexError {}

impl LineIndex {
    /// Build from `source` bytes. The index does not hold the
    /// reference — it captures only the newline offsets.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut line_starts: Vec<u32> = Vec::with_capacity(source.len() / 40);
        line_starts.push(0);
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                let next = u32::try_from(i.saturating_add(1)).unwrap_or(u32::MAX);
                line_starts.push(next);
            }
        }
        Self { line_starts }
    }

    /// 1-indexed (line, column) for the codepoint starting at `byte`.
    ///
    /// `source` must be the same buffer the index was built from
    /// (otherwise codepoint counting will use the wrong bytes).
    ///
    /// # Errors
    ///
    /// Returns `Err` if `byte` lies past the end of `source` or not
    /// on a UTF-8 boundary. Callers should pass offsets produced by
    /// pulldown-cmark, which always satisfy both conditions.
    pub fn locate(&self, source: &str, byte: usize) -> Result<(usize, usize), LineIndexError> {
        if byte > source.len() {
            return Err(LineIndexError::OffsetOutOfBounds {
                byte,
                source_len: source.len(),
            });
        }
        let byte_u32 = u32::try_from(byte).map_err(|_| LineIndexError::OffsetTooLarge { byte })?;
        // Binary search for the largest line_start ≤ byte.
        let idx = match self.line_starts.binary_search(&byte_u32) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(idx).copied().unwrap_or(0) as usize;
        let prefix = source
            .get(line_start..byte)
            .ok_or(LineIndexError::NotUtf8Boundary { byte })?;
        let column = prefix.chars().count().saturating_add(1);
        Ok((idx.saturating_add(1), column))
    }

    /// Byte offset of the codepoint at LSP-convention `(line, column)`
    /// (both 0-indexed; column counts Unicode codepoints).
    ///
    /// Returns `None` if `line` is past the end of the source, if
    /// `column` lands past the end of its line, or if the position
    /// isn't on a UTF-8 boundary.
    ///
    /// The 0-based input convention matches the Language Server
    /// Protocol (`Position { line, character }`); the existing
    /// [`Self::locate`] reverses the mapping and returns 1-based
    /// coordinates suitable for `file:line:col` diagnostic display.
    /// Don't conflate the two.
    #[must_use]
    pub fn byte_of_position_0based(&self, source: &str, line: usize, column: usize) -> Option<usize> {
        let line_start = self.line_starts.get(line).copied()? as usize;
        let line_end = self
            .line_starts
            .get(line.saturating_add(1))
            .copied()
            .map_or(source.len(), |n| n as usize);
        // Strip the trailing newline (and an optional preceding `\r`)
        // so a request for column == line length resolves at the byte
        // before the line terminator rather than past it.
        let mut visible_end = line_end;
        if visible_end > line_start && source.as_bytes().get(visible_end.saturating_sub(1)) == Some(&b'\n') {
            visible_end = visible_end.saturating_sub(1);
            if visible_end > line_start && source.as_bytes().get(visible_end.saturating_sub(1)) == Some(&b'\r') {
                visible_end = visible_end.saturating_sub(1);
            }
        }
        let line_text = source.get(line_start..visible_end)?;
        let mut cursor = line_start;
        let mut remaining = column;
        for ch in line_text.chars() {
            if remaining == 0 {
                return Some(cursor);
            }
            cursor = cursor.saturating_add(ch.len_utf8());
            remaining = remaining.saturating_sub(1);
        }
        // Column lands at — or past — the end of the visible line text.
        // LSP convention permits "one past last codepoint" (end-of-line
        // cursor), so accept column == codepoint_count by returning the
        // visible-end byte. Larger columns are out of range.
        (remaining == 0).then_some(visible_end)
    }

    /// Byte range of the line containing `byte`, with the trailing
    /// `\n` trimmed. Returns `None` if `byte` is past the source end.
    ///
    /// The slice `source[range]` is exactly the line text the
    /// rustc-style pretty renderer and the JSON Lines `snippet` field
    /// quote back to the user.
    #[must_use]
    pub fn line_bounds(&self, source: &str, byte: usize) -> Option<std::ops::Range<usize>> {
        if byte > source.len() {
            return None;
        }
        let byte_u32 = u32::try_from(byte).ok()?;
        let idx = match self.line_starts.binary_search(&byte_u32) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let start = self.line_starts.get(idx).copied()? as usize;
        let raw_end = self
            .line_starts
            .get(idx.saturating_add(1))
            .copied()
            .map_or(source.len(), |n| n as usize);
        // Trim the trailing `\n` (and a preceding `\r`, if present)
        // so callers get just the visible line text.
        let mut end = raw_end;
        if end > start && source.as_bytes().get(end.saturating_sub(1)) == Some(&b'\n') {
            end = end.saturating_sub(1);
            if end > start && source.as_bytes().get(end.saturating_sub(1)) == Some(&b'\r') {
                end = end.saturating_sub(1);
            }
        }
        Some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::LineIndex;

    fn locate(idx: &LineIndex, source: &str, byte: usize) -> Result<(usize, usize), String> {
        idx.locate(source, byte).map_err(|err| err.to_string())
    }

    #[test]
    fn locate_start_of_first_line() -> Result<(), String> {
        let src = "hello\nworld\n";
        let idx = LineIndex::new(src);
        assert_eq!(locate(&idx, src, 0)?, (1, 1));
        Ok(())
    }

    #[test]
    fn locate_after_newline() -> Result<(), String> {
        let src = "hello\nworld\n";
        let idx = LineIndex::new(src);
        assert_eq!(locate(&idx, src, 6)?, (2, 1));
        Ok(())
    }

    #[test]
    fn locate_codepoint_column() -> Result<(), String> {
        let src = "αβγ\n";
        let idx = LineIndex::new(src);
        assert_eq!(locate(&idx, src, 4)?, (1, 3));
        Ok(())
    }

    #[test]
    fn rejects_out_of_range() {
        let src = "hi\n";
        let idx = LineIndex::new(src);
        assert!(idx.locate(src, 99).is_err());
    }

    #[test]
    fn byte_of_position_0based_basic() {
        let src = "hello\nworld\n";
        let idx = LineIndex::new(src);
        assert_eq!(idx.byte_of_position_0based(src, 0, 0), Some(0));
        assert_eq!(idx.byte_of_position_0based(src, 0, 5), Some(5));
        assert_eq!(idx.byte_of_position_0based(src, 1, 0), Some(6));
        assert_eq!(idx.byte_of_position_0based(src, 1, 5), Some(11));
    }

    #[test]
    fn byte_of_position_0based_codepoints() {
        let src = "αβγ\n";
        let idx = LineIndex::new(src);
        assert_eq!(idx.byte_of_position_0based(src, 0, 0), Some(0));
        assert_eq!(idx.byte_of_position_0based(src, 0, 1), Some(2));
        assert_eq!(idx.byte_of_position_0based(src, 0, 2), Some(4));
        assert_eq!(idx.byte_of_position_0based(src, 0, 3), Some(6));
    }

    #[test]
    fn byte_of_position_0based_out_of_range() {
        let src = "hi\n";
        let idx = LineIndex::new(src);
        assert!(idx.byte_of_position_0based(src, 5, 0).is_none());
        assert!(idx.byte_of_position_0based(src, 0, 99).is_none());
    }
}
