//! Diagnostics emitted by lint rules.

use std::borrow::Cow;
use std::ops::Range;

use crate::document::Document;

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
        doc: &Document<'_>,
        byte_offset: usize,
        local: Range<usize>,
        message: String,
        fix: Option<Fix>,
    ) -> Option<Self> {
        let start = byte_offset.saturating_add(local.start);
        let end = byte_offset.saturating_add(local.end);
        let (line, column) = doc.line_index().locate(start).ok()?;
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
}
