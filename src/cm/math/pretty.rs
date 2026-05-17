//! Math-span pretty-printer.
//!
//! Each [`MathSpan`] variant has an inherent `pretty()` method that
//! renders the region as a normalised [`Doc`]. The dispatcher in
//! [`MathSpan::pretty`] picks the variant-specific helper; verbatim
//! mode short-circuits to the source bytes; brace-imbalanced bodies
//! also fall back to verbatim (we cannot safely normalise content
//! whose grouping we cannot trust).
//!
//! The three normalisations applied in `Normalise` mode are:
//!
//! 1. Inline whitespace collapse — runs of `[' ' '\t']` inside `\(…\)`
//!    or `$…$` collapse to a single space, leading/trailing space is
//!    trimmed. LaTeX escape sequences (`\,`, `\ `, `\;`, `\!`) are
//!    preserved byte-for-byte.
//! 2. Display layout — the opener / closer move to their own lines
//!    around the body so the math reads as a visual block.
//! 3. Ampersand alignment — aligning environments (`align`, matrix
//!    family, `cases`, …) get per-column padding so `&` separators
//!    line up vertically using Unicode display width.

use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use crate::format::doc::{Doc, concat, hard_line, text, unbreakable};
use crate::format::pretty::PrettyCtx;

use super::env::EnvKind;
use super::span::{DisplayDelim, InlineDelim, MathSpan};

impl MathSpan {
    /// Render this math region as a [`Doc`]. The body is sourced
    /// through [`super::span::MathBody::as_str`] (container prefixes
    /// stripped) so the surrounding [`Doc::Prefix`] cascade can re-add
    /// blockquote `>` markers and list-item indent without doubling.
    ///
    /// Display and environment math whose source neighbours are prose
    /// on the same line (e.g. `... the operation \[ A ... \] is
    /// stable.`) are emitted byte-verbatim from source as a single
    /// [`Doc::Text`]. The author treated the delimiters as literal
    /// text; breaking the source bytes onto separate lines would
    /// rewrite the paragraph's HTML. Standalone display / environment
    /// math (the math is the only non-whitespace content on its
    /// source lines, modulo container prefixes) is rendered with
    /// hard-line boundaries around the body so the layout reads as a
    /// visual block.
    ///
    /// `region` is the **outer** range (including delimiter tokens).
    pub(crate) fn pretty<'a>(&self, ctx: &PrettyCtx<'a>, region: &Range<usize>) -> Doc<'a> {
        let source = ctx.source;
        let normalise = ctx.opts.mode() != crate::config::FormatMode::Verbatim && ctx.opts.math().normalise;
        let body = self.body().as_str(source);
        let has_transparent = self.body().has_transparent_runs();

        // Display / environment math whose source neighbours are
        // prose (no transparent runs, opener / closer are mid-line)
        // is emitted source-byte-verbatim. The author wrote it as
        // inline text; pulldown parsed it as such; we preserve the
        // exact bytes so HTML stays identical.
        if matches!(self, Self::Display { .. } | Self::Environment { .. })
            && !has_transparent
            && !is_line_standalone(source, region)
        {
            let slice = source.get(region.clone()).unwrap_or("");
            return text(slice.to_owned());
        }

        if normalise && body_braces_balanced(body.as_ref()).is_err() {
            // Brace-imbalanced bodies cannot be safely normalised
            // (whitespace collapse / column padding might cross a
            // half-open group). Fall through to the verbatim path,
            // which emits body bytes unchanged.
            return self.pretty_verbatim(body.as_ref(), source);
        }

        match self {
            Self::Inline { delim, .. } => {
                if normalise {
                    unbreakable(pretty_inline(*delim, body.as_ref()))
                } else {
                    unbreakable(verbatim_inline(*delim, body.as_ref()))
                }
            }
            Self::Display { delim, .. } => {
                if normalise {
                    pretty_display(*delim, body.as_ref())
                } else {
                    verbatim_display(*delim, body.as_ref())
                }
            }
            Self::Environment { env, .. } => {
                if normalise {
                    pretty_env(env, body.as_ref(), source)
                } else {
                    verbatim_env(env, body.as_ref(), source)
                }
            }
        }
    }

    /// Verbatim fallback used when the normalisation path detects
    /// brace imbalance. Emits the variant's canonical structure with
    /// body bytes unmodified.
    fn pretty_verbatim<'a>(&self, body: &str, source: &str) -> Doc<'a> {
        match self {
            Self::Inline { delim, .. } => unbreakable(verbatim_inline(*delim, body)),
            Self::Display { delim, .. } => verbatim_display(*delim, body),
            Self::Environment { env, .. } => verbatim_env(env, body, source),
        }
    }
}

/// `true` iff the math region's opener and closer are line-standalone
/// in source: only whitespace (or nothing) sits between the opener
/// and the previous line break, and between the closer and the next
/// line break. Used by `MathSpan::pretty` to decide between
/// structural layout and source-byte-verbatim emission for display
/// and environment math.
///
/// Container-nested math (whose body carries transparent runs) is
/// treated as standalone by `MathSpan::pretty` directly — this helper
/// is only consulted when no transparent runs exist, so the simple
/// "ASCII whitespace until line boundary" check is sufficient.
fn is_line_standalone(source: &str, region: &Range<usize>) -> bool {
    let bytes = source.as_bytes();
    let leading_ok = {
        let mut j = region.start;
        loop {
            if j == 0 {
                break true;
            }
            let prev = j.saturating_sub(1);
            match bytes.get(prev).copied() {
                Some(b'\n') => break true,
                Some(b' ' | b'\t') => j = prev,
                _ => break false,
            }
        }
    };
    if !leading_ok {
        return false;
    }
    let mut j = region.end;
    loop {
        match bytes.get(j).copied() {
            None | Some(b'\n') => return true,
            Some(b' ' | b'\t') => j = j.saturating_add(1),
            _ => return false,
        }
    }
}

/// Verbatim inline math: concatenate opener + body + closer with no
/// whitespace transformation. The body is the cleaned [`MathBody::as_str`]
/// content (container prefixes already stripped).
fn verbatim_inline<'a>(delim: InlineDelim, body: &str) -> Doc<'a> {
    concat([text(delim.open()), text(body.to_owned()), text(delim.close())])
}

/// Verbatim display math. The body is split at `\n` so the surrounding
/// `Doc::Prefix` cascade re-applies container prefixes per line; leading
/// and trailing newlines are trimmed because they are already encoded
/// by the structural `hard_line` separators around the body.
fn verbatim_display<'a>(delim: DisplayDelim, body: &str) -> Doc<'a> {
    let trimmed = body.trim_matches('\n');
    unbreakable(concat([
        text(delim.open()),
        hard_line(),
        text(trimmed.to_owned()),
        hard_line(),
        text(delim.close()),
    ]))
}

/// Verbatim environment. Same shape as [`verbatim_display`], with the
/// `\begin{name}` / `\end{name}` tokens computed from the resolved
/// environment name.
fn verbatim_env<'a>(env: &EnvKind, body: &str, source: &str) -> Doc<'a> {
    let name = env.name(source).to_owned();
    let trimmed = body.trim_matches('\n');
    unbreakable(concat([
        text(format!("\\begin{{{name}}}")),
        hard_line(),
        text(trimmed.to_owned()),
        hard_line(),
        text(format!("\\end{{{name}}}")),
    ]))
}

/// Walk `body` and confirm `{` / `}` balance. `\{` and `\}` are
/// escapes and do not count. Returns the byte offset of the first
/// offending byte on failure (either an unmatched `}` or the body
/// start when the body ends mid-group). Shared with the scanner so
/// the lint rule and the pretty-printer agree on what "balanced"
/// means.
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

/// Render an inline math region as a `Doc`. The body has its inline
/// whitespace collapsed and the markers are emitted with no padding.
fn pretty_inline<'a>(delim: InlineDelim, body: &str) -> Doc<'a> {
    let normalised = collapse_inline_ws(body);
    let trimmed = normalised.trim();
    concat([text(delim.open()), text(trimmed.to_owned()), text(delim.close())])
}

/// Render a display math region. The body keeps its newlines but
/// trailing whitespace on each line is trimmed; the opener and
/// closer occupy their own lines.
fn pretty_display<'a>(delim: DisplayDelim, body: &str) -> Doc<'a> {
    let trimmed_body = trim_display_body(body);
    unbreakable(concat([
        text(delim.open()),
        hard_line(),
        text(trimmed_body),
        hard_line(),
        text(delim.close()),
    ]))
}

/// Render an environment. Aligning environments get column-padded
/// rows; non-aligning environments emit `\begin{name}` and
/// `\end{name}` on their own lines with the body verbatim.
fn pretty_env<'a>(env: &EnvKind, body: &str, source: &str) -> Doc<'a> {
    let name = env.name(source).to_owned();
    let body_rendered = if env.is_aligning() {
        align_env_body(body)
    } else {
        trim_display_body(body)
    };
    unbreakable(concat([
        text(format!("\\begin{{{name}}}")),
        hard_line(),
        text(body_rendered),
        hard_line(),
        text(format!("\\end{{{name}}}")),
    ]))
}

/// Collapse runs of ` ` / `\t` to a single space outside LaTeX escape
/// sequences. `\,`, `\ `, `\;`, `\!` and other backslash escapes are
/// preserved verbatim — they're TeX-significant.
fn collapse_inline_ws(body: &str) -> String {
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
        // Newlines in inline math are invalid LaTeX; preserve them so
        // the author sees their bug rather than have us silently
        // hide it.
        in_ws = false;
        out.push(b as char);
        i = i.saturating_add(1);
    }
    out
}

/// For display bodies: trim each line of trailing whitespace and trim
/// leading/trailing blank lines, but otherwise preserve line structure
/// (display math is multi-line by convention).
fn trim_display_body(body: &str) -> String {
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
fn align_env_body(body: &str) -> String {
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
            // Pad all but the last column so trailing `&`s line up.
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

/// Split an environment body on unescaped `\\` row separators. The
/// scanner has already trimmed the surrounding `\begin{…}` /
/// `\end{…}` tokens; what's left is the raw row stream.
fn split_rows(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut rows: Vec<&str> = Vec::new();
    let mut last = 0usize;
    let mut i = 0usize;
    while let Some(b) = bytes.get(i).copied() {
        let b2 = bytes.get(i.saturating_add(1)).copied();
        if b == b'\\' && b2 == Some(b'\\') {
            // Must not be inside a longer backslash run; check the
            // preceding byte to confirm this pair is unescaped.
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
#[allow(clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn collapse_inline_ws_collapses_runs() {
        assert_eq!(collapse_inline_ws("x  +   y"), "x + y");
    }

    #[test]
    fn collapse_inline_ws_preserves_thin_space() {
        assert_eq!(collapse_inline_ws(r"a\,b"), r"a\,b");
        assert_eq!(collapse_inline_ws(r"a\ b"), r"a\ b");
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
    fn body_braces_balanced_ignores_escapes() {
        assert!(body_braces_balanced(r"\{a\}").is_ok());
    }

    #[test]
    fn split_rows_basic() {
        // split_rows preserves intra-row spacing; per-cell trim happens
        // inside align_env_body once cells are extracted.
        assert_eq!(split_rows(r"a \\ b \\ c"), vec!["a ", " b ", " c"]);
    }

    #[test]
    fn split_cells_basic() {
        assert_eq!(split_cells("a & b & c"), vec!["a ", " b ", " c"]);
    }

    #[test]
    fn align_env_body_pads_ascii_columns() {
        let body = "a & 1 \\\\ longer & 22";
        let out = align_env_body(body);
        // Column 0 widths: "a" (1) and "longer" (6) → pad first cell.
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("a     "));
        assert!(lines[1].starts_with("longer"));
        assert!(lines[0].ends_with("& 1 \\\\") || lines[0].contains("& 1"));
    }

    #[test]
    fn align_env_body_uses_unicode_width() {
        // Cyrillic and Greek letters are single-width; the wider cell
        // dictates padding.
        let body = "α & β \\\\ γγγ & δ";
        let out = align_env_body(body);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].starts_with("γγγ"));
    }
}
