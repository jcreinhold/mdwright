//! Inline HTML spans (CM §6.6).
//!
//! Pulldown-cmark runs the CM §6.6 regex before emitting
//! `Event::InlineHtml`, so the bytes inside an [`InlineHtmlSpan`] are
//! evidence the parser classified them. The constructor stores them
//! and applies one source-position fix-up: a comment that sat on its
//! own line with ≥ 4 columns of leading whitespace must retain that
//! indent on output, or CM §4.6 rule type-2 would re-parse the
//! comment as an HTML block and split the surrounding paragraph.

use std::borrow::Cow;

/// One inline HTML span; the stored bytes are emission-ready under any
/// CM-compliant tokenizer.
#[derive(Clone, Debug)]
pub struct InlineHtmlSpan<'a> {
    bytes: Cow<'a, str>,
}

impl<'a> InlineHtmlSpan<'a> {
    /// Store `raw` as inline HTML, prepending a 4-space indent when
    /// the span is a comment that the source placed on its own line
    /// with ≥ 4 columns of indent. `src_start` is the byte offset in
    /// `source` where the span begins.
    #[tracing::instrument(level = "trace", skip(raw, source))]
    pub(crate) fn from_parser(raw: Cow<'a, str>, src_start: usize, source: &str) -> Self {
        if raw.starts_with("<!--") && comment_indented_on_own_line(source, src_start) {
            let mut joined = String::with_capacity(raw.len().saturating_add(4));
            joined.push_str("    ");
            joined.push_str(raw.as_ref());
            return Self {
                bytes: Cow::Owned(joined),
            };
        }
        Self { bytes: raw }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.bytes
    }

    /// Emit the raw HTML span as an `unbreakable` doc.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self) -> crate::format::doc::Doc<'b> {
        use crate::format::doc::{text, unbreakable};
        unbreakable(text(self.bytes.as_ref().to_owned()))
    }
}

/// True when the source placed `start` at the beginning of a line
/// whose leading whitespace is at least four columns wide. Mirrors
/// the prior `comment_indented_on_own_line_in_source` helper in
/// `format/inline.rs`.
fn comment_indented_on_own_line(source: &str, start: usize) -> bool {
    if start == 0 {
        return false;
    }
    let prefix = source.get(..start).unwrap_or("");
    let Some(nl) = prefix.rfind('\n') else {
        return false;
    };
    let Some(line_lead) = prefix.get(nl.saturating_add(1)..) else {
        return false;
    };
    line_lead.len() >= 4 && line_lead.bytes().all(|b| matches!(b, b' ' | b'\t'))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn non_comment_is_passed_through() {
        let span = InlineHtmlSpan::from_parser(Cow::Borrowed("<span>"), 0, "<span>");
        assert_eq!(span.as_str(), "<span>");
    }

    #[test]
    fn comment_at_line_start_without_indent_is_passed_through() {
        let source = "before\n<!-- c -->";
        let start = source.find("<!--").unwrap();
        let span = InlineHtmlSpan::from_parser(Cow::Borrowed("<!-- c -->"), start, source);
        assert_eq!(span.as_str(), "<!-- c -->");
    }

    #[test]
    fn comment_on_own_line_with_indent_gains_prefix() {
        let source = "before\n    <!-- c -->\nafter";
        let start = source.find("<!--").unwrap();
        let span = InlineHtmlSpan::from_parser(Cow::Borrowed("<!-- c -->"), start, source);
        assert_eq!(span.as_str(), "    <!-- c -->");
    }

    #[test]
    fn comment_mid_line_is_passed_through() {
        let source = "text <!-- c --> tail";
        let start = source.find("<!--").unwrap();
        let span = InlineHtmlSpan::from_parser(Cow::Borrowed("<!-- c -->"), start, source);
        assert_eq!(span.as_str(), "<!-- c -->");
    }
}
