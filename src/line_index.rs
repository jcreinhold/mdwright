//! Byte-offset → (line, column) mapping for a single source string.
//!
//! pulldown-cmark hands us byte ranges; lint diagnostics report
//! `file:line:col` in the editor convention (both 1-indexed, columns
//! counted in Unicode codepoints, not bytes). This helper builds the
//! line-start table once per document and answers every offset lookup
//! in O(log n).

use anyhow::{Result, bail};

/// Maps byte offsets in a source string to 1-indexed (line, column)
/// pairs. Columns count Unicode codepoints, matching what `grep -n`
/// and most editors display.
#[derive(Debug)]
pub struct LineIndex<'a> {
    source: &'a str,
    /// `line_starts[i]` is the byte offset of the first byte of line
    /// `i + 1`. `line_starts[0]` is always `0`. The table has one
    /// entry per newline plus one for the document start, so a final
    /// non-terminated line still has a start entry.
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    #[must_use]
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = Vec::with_capacity(source.len() / 40);
        line_starts.push(0);
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i.saturating_add(1));
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    /// 1-indexed (line, column) for the codepoint starting at `byte`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `byte` lies past the end of the source or not
    /// on a UTF-8 boundary. Callers should pass offsets produced by
    /// pulldown-cmark, which always satisfy both conditions.
    pub fn locate(&self, byte: usize) -> Result<(usize, usize)> {
        if byte > self.source.len() {
            bail!(
                "byte offset {byte} past source length {}",
                self.source.len()
            );
        }
        // Binary search for the largest line_start ≤ byte.
        let idx = match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(idx).copied().unwrap_or(0);
        let prefix = self
            .source
            .get(line_start..byte)
            .ok_or_else(|| anyhow::anyhow!("byte {byte} not on UTF-8 boundary"))?;
        let column = prefix.chars().count().saturating_add(1);
        Ok((idx.saturating_add(1), column))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::LineIndex;

    #[test]
    fn locate_start_of_first_line() -> Result<()> {
        let idx = LineIndex::new("hello\nworld\n");
        assert_eq!(idx.locate(0)?, (1, 1));
        Ok(())
    }

    #[test]
    fn locate_after_newline() -> Result<()> {
        let idx = LineIndex::new("hello\nworld\n");
        assert_eq!(idx.locate(6)?, (2, 1));
        Ok(())
    }

    #[test]
    fn locate_codepoint_column() -> Result<()> {
        // `αβ` is 4 bytes (2 chars × 2 bytes), so byte 4 is the start
        // of the third codepoint on line 1.
        let idx = LineIndex::new("αβγ\n");
        assert_eq!(idx.locate(4)?, (1, 3));
        Ok(())
    }

    #[test]
    fn rejects_out_of_range() {
        let idx = LineIndex::new("hi\n");
        assert!(idx.locate(99).is_err());
    }
}
