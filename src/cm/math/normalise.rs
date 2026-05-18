//! Pure string-to-string math-body normalisations.
//!
//! Lifted out of the deleted pretty-printer (`super::pretty`) so the
//! canonicalise pass at `crate::format::canonicalise::rewrite_math`
//! can apply align-environment column padding and display/inline
//! whitespace normalisation against rendered output bytes without
//! routing through the Doc IR.
//!
//! Every function in this module takes a body slice and returns an
//! owned `String`. The opener / closer reconstruction lives in the
//! canonicalise caller — these helpers are pure body transforms.
//!
//! Brace-balance probing is shared with [`super::scan`] so the
//! lint rule, the scanner, and the canonicalise gate agree on what
//! "balanced" means; see [`body_braces_balanced`].

#![allow(dead_code)]
use unicode_width::UnicodeWidthStr;

/// Walk `body` and confirm `{` / `}` balance. `\{` and `\}` are
/// escapes and do not count. Returns the byte offset of the first
/// offending byte on failure (either an unmatched `}` or the body
/// start when the body ends mid-group).
pub(crate) fn body_braces_balanced(body: &str) -> Result<(), usize> {
    let bytes = body.as_bytes();
    let mut depth: i64 = 0;
    let mut i = 0usize;
    while let Some(b) = bytes.get(i).copied() {
        if b == b'\\' {
            if matches!(bytes.get(i.saturating_add(1)).copied(), Some(b'{' | b'}')) {
                i = i.saturating_add(2);
                continue;
            }
        } else if b == b'{' {
            depth = depth.saturating_add(1);
        } else if b == b'}' {
            depth = depth.saturating_sub(1);
            if depth < 0 {
                return Err(i);
            }
        }
        i = i.saturating_add(1);
    }
    if depth == 0 { Ok(()) } else { Err(0) }
}

/// Collapse runs of ` ` / `\t` to a single space outside LaTeX escape
/// sequences. `\,`, `\ `, `\;`, `\!` and other backslash escapes are
/// preserved verbatim.
pub(crate) fn collapse_inline_ws(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0usize;
    let mut in_ws = false;
    while let Some(b) = bytes.get(i).copied() {
        if b == b'\\' {
            in_ws = false;
            out.push('\\');
            if let Some(next) = bytes.get(i.saturating_add(1)).copied() {
                out.push(next as char);
                i = i.saturating_add(2);
            } else {
                i = i.saturating_add(1);
            }
            continue;
        }
        if b == b' ' || b == b'\t' {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
            i = i.saturating_add(1);
            continue;
        }
        in_ws = false;
        out.push(b as char);
        i = i.saturating_add(1);
    }
    out
}

/// For display bodies: trim each line of trailing whitespace and trim
/// leading/trailing blank lines.
pub(crate) fn trim_display_body(body: &str) -> String {
    let mut lines: Vec<&str> = body.lines().map(str::trim_end).collect();
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Lay out an aligning-environment body: split on `\\` rows, then on
/// `&` cells, pad each cell to its column's max display width, emit
/// with `" & "` separators and a `" \\\n"` row break.
pub(crate) fn align_env_body(body: &str) -> String {
    let raw_rows = split_rows(body);
    if raw_rows.is_empty() {
        return String::new();
    }
    let rows: Vec<Vec<String>> = raw_rows
        .iter()
        .map(|row| split_cells(row).into_iter().map(|c| c.trim().to_owned()).collect())
        .collect();

    let n_cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths: Vec<usize> = vec![0; n_cols];
    for row in &rows {
        for (j, cell) in row.iter().enumerate() {
            let w = UnicodeWidthStr::width(cell.as_str());
            if let Some(slot) = widths.get_mut(j)
                && w > *slot
            {
                *slot = w;
            }
        }
    }

    let mut out = String::with_capacity(body.len());
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push_str(" \\\\\n");
        }
        let last_j = row.len().saturating_sub(1);
        for (j, cell) in row.iter().enumerate() {
            if j > 0 {
                out.push_str(" & ");
            }
            out.push_str(cell);
            if j < last_j {
                let w = UnicodeWidthStr::width(cell.as_str());
                let pad = widths.get(j).copied().unwrap_or(0).saturating_sub(w);
                for _ in 0..pad {
                    out.push(' ');
                }
            }
        }
    }
    out
}

/// Split an environment body on unescaped `\\` row separators.
fn split_rows(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut rows: Vec<&str> = Vec::new();
    let mut last = 0usize;
    let mut i = 0usize;
    while let Some(b) = bytes.get(i).copied() {
        let b2 = bytes.get(i.saturating_add(1)).copied();
        if b == b'\\' && b2 == Some(b'\\') {
            let prev = i.checked_sub(1).and_then(|p| bytes.get(p).copied());
            if prev != Some(b'\\') {
                let segment = body.get(last..i).unwrap_or("").trim_matches('\n');
                rows.push(segment);
                last = i.saturating_add(2);
                i = last;
                continue;
            }
        }
        i = i.saturating_add(1);
    }
    let tail = body.get(last..).unwrap_or("").trim_matches('\n');
    if !tail.is_empty() {
        rows.push(tail);
    }
    rows
}

/// Split a row on unescaped `&` column separators.
fn split_cells(row: &str) -> Vec<&str> {
    let bytes = row.as_bytes();
    let mut cells: Vec<&str> = Vec::new();
    let mut last = 0usize;
    let mut i = 0usize;
    while let Some(b) = bytes.get(i).copied() {
        let prev = i.checked_sub(1).and_then(|p| bytes.get(p).copied());
        if b == b'&' && prev != Some(b'\\') {
            cells.push(row.get(last..i).unwrap_or(""));
            last = i.saturating_add(1);
        }
        i = i.saturating_add(1);
    }
    cells.push(row.get(last..).unwrap_or(""));
    cells
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn collapse_inline_ws_collapses_runs() {
        assert_eq!(collapse_inline_ws("x  +   y"), "x + y");
    }

    #[test]
    fn collapse_inline_ws_preserves_thin_space() {
        assert_eq!(collapse_inline_ws(r"a\,b"), r"a\,b");
    }

    #[test]
    fn body_braces_balanced_accepts_matched() {
        assert!(body_braces_balanced("a{b{c}d}e").is_ok());
    }

    #[test]
    fn body_braces_balanced_rejects_unmatched() {
        assert!(body_braces_balanced("a}").is_err());
        assert!(body_braces_balanced("a{b").is_err());
    }

    #[test]
    fn align_pads_columns() {
        let out = align_env_body("x &= a + b \\\\ longvar &= cc");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("x       "), "got {:?}", lines[0]);
        assert!(lines[1].starts_with("longvar"), "got {:?}", lines[1]);
    }
}
