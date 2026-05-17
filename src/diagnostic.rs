//! Diagnostics emitted by lint rules.

use std::borrow::Cow;
use std::ops::Range;

use crate::document::Document;
use crate::line_index::LineIndex;

/// Diagnostic severity for serialised output.
///
/// `Error` is the default for non-advisory rules; `Advisory` reflects
/// [`Diagnostic::advisory`]. `Warning` is reserved for future use
/// (no current rule emits it) and is included so the JSON Lines
/// schema can name all three levels up front.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Advisory,
}

impl Severity {
    /// Lowercase string form used by the v2 JSON Lines emitter and
    /// the rustc-style pretty header.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Advisory => "advisory",
        }
    }
}

/// Source-snippet view shared by the pretty and JSON renderers:
/// the one line of source covering a diagnostic's span, plus the
/// column range of the underlined region.
///
/// For multi-line spans the underlined region is clamped to the
/// first line — both renderers point at the start of the offence.
#[derive(Clone, Debug)]
pub struct Snippet<'a> {
    /// 1-indexed line number.
    pub line_no: u32,
    /// 1-indexed codepoint column of the span's first character.
    pub col_start: u32,
    /// 1-indexed codepoint column one past the span's last character
    /// on this line (so `col_end - col_start` codepoints are
    /// underlined). Always ≥ `col_start + 1` so the caret is visible.
    pub col_end: u32,
    /// The line text, without the trailing `\n`.
    pub line_text: &'a str,
}

impl<'a> Snippet<'a> {
    /// Build a snippet for `span` inside `source`. Returns `None` if
    /// `span.start` lies outside `source` or off a UTF-8 boundary —
    /// the same fallback as [`Diagnostic::at`].
    #[must_use]
    pub fn from_span(line_index: &LineIndex, source: &'a str, span: &Range<usize>) -> Option<Self> {
        let (line_no_usize, col_start_usize) = line_index.locate(source, span.start).ok()?;
        let line_no = u32::try_from(line_no_usize).ok()?;
        let col_start = u32::try_from(col_start_usize).ok()?;
        let bounds = line_index.line_bounds(source, span.start)?;
        let line_text = source.get(bounds.clone())?;
        let end_on_line = span.end.min(bounds.end);
        let after = source.get(bounds.start..end_on_line)?;
        let after_cols = u32::try_from(after.chars().count().saturating_add(1)).ok()?;
        let col_end = after_cols.max(col_start.saturating_add(1));
        Some(Self {
            line_no,
            col_start,
            col_end,
            line_text,
        })
    }
}

/// One issue at one source location, optionally with an automatic
/// [`Fix`]. Spans are byte ranges into the original source string.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Kebab-case identifier of the rule that produced this
    /// diagnostic. `Cow` so stdlib rules can borrow `&'static str`
    /// names while user rules with runtime-built names own the buffer.
    /// The dispatcher stamps this field after each rule's `check`
    /// returns, so rule implementations do not set it.
    pub rule: Cow<'static, str>,
    /// 1-indexed line number of the diagnostic's first byte.
    pub line: usize,
    /// 1-indexed codepoint column.
    pub column: usize,
    /// Byte span within the source. `source.get(span.clone())` is the
    /// substring the diagnostic refers to.
    pub span: Range<usize>,
    /// One-line human-readable message.
    pub message: String,
    /// Optional replacement covering `span`.
    pub fix: Option<Fix>,
    /// Whether this diagnostic is advisory (informational; does not
    /// fail `--check`). Set by the dispatcher from the rule's
    /// `is_advisory()`.
    pub advisory: bool,
}

#[derive(Clone, Debug)]
pub struct Fix {
    pub replacement: String,
    /// Whether the fix can be applied without manual review. `false`
    /// fixes are surfaced as suggestions only, never under `--fix`.
    pub safe: bool,
}

impl Diagnostic {
    /// Build a diagnostic at a position within a borrowed source
    /// slice. `byte_offset` is the absolute offset of the slice's
    /// first byte; `local` is the match range within that slice.
    ///
    /// Returns `None` if the line-index lookup fails — never observed
    /// for offsets produced by pulldown-cmark, but the safe-fallback
    /// behaviour is to drop the diagnostic rather than panic.
    /// The dispatcher fills in `rule` and `advisory` after the
    /// containing rule's `check` returns.
    #[must_use]
    pub fn at(
        doc: &Document,
        byte_offset: usize,
        local: Range<usize>,
        message: String,
        fix: Option<Fix>,
    ) -> Option<Self> {
        let start = byte_offset.saturating_add(local.start);
        let end = byte_offset.saturating_add(local.end);
        let (line, column) = doc.line_index().locate(doc.source(), start).ok()?;
        Some(Self {
            rule: Cow::Borrowed(""),
            line,
            column,
            span: start..end,
            message,
            fix,
            advisory: false,
        })
    }

    /// Suppression marker text. The Markdown comment for muting this
    /// diagnostic on the next block is
    /// `<!-- mdwright: allow rule-name -->`.
    #[must_use]
    pub fn suppress_via(&self) -> String {
        format!("mdwright: allow {}", self.rule)
    }

    /// Diagnostic severity derived from [`Self::advisory`]. Used by
    /// the v2 JSON Lines emitter and the rustc-style pretty header.
    #[must_use]
    pub fn severity(&self) -> Severity {
        if self.advisory {
            Severity::Advisory
        } else {
            Severity::Error
        }
    }
}
