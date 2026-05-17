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

use anyhow::{Result, bail};

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
    pub fn locate(&self, source: &str, byte: usize) -> Result<(usize, usize)> {
        if byte > source.len() {
            bail!("byte offset {byte} past source length {}", source.len());
        }
        let byte_u32 = u32::try_from(byte).map_err(|_| anyhow::anyhow!("byte offset > u32"))?;
        // Binary search for the largest line_start ≤ byte.
        let idx = match self.line_starts.binary_search(&byte_u32) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(idx).copied().unwrap_or(0) as usize;
        let prefix = source
            .get(line_start..byte)
            .ok_or_else(|| anyhow::anyhow!("byte {byte} not on UTF-8 boundary"))?;
        let column = prefix.chars().count().saturating_add(1);
        Ok((idx.saturating_add(1), column))
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
    use anyhow::Result;

    use super::LineIndex;

    #[test]
    fn locate_start_of_first_line() -> Result<()> {
        let src = "hello\nworld\n";
        let idx = LineIndex::new(src);
        assert_eq!(idx.locate(src, 0)?, (1, 1));
        Ok(())
    }

    #[test]
    fn locate_after_newline() -> Result<()> {
        let src = "hello\nworld\n";
        let idx = LineIndex::new(src);
        assert_eq!(idx.locate(src, 6)?, (2, 1));
        Ok(())
    }

    #[test]
    fn locate_codepoint_column() -> Result<()> {
        let src = "αβγ\n";
        let idx = LineIndex::new(src);
        assert_eq!(idx.locate(src, 4)?, (1, 3));
        Ok(())
    }

    #[test]
    fn rejects_out_of_range() {
        let src = "hi\n";
        let idx = LineIndex::new(src);
        assert!(idx.locate(src, 99).is_err());
    }
}
