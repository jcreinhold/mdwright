//! Trailing `.` or `:` on an ATX or setext heading.
//!
//! Headings are titles, not sentences. A trailing period reads as
//! a stray dot; a trailing colon usually means the author wanted a
//! list or paragraph instead.

use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct HeadingPunctuation;

impl LintRule for HeadingPunctuation {
    fn name(&self) -> &str {
        "heading-punctuation"
    }

    fn description(&self) -> &str {
        "Trailing `.` or `:` on a heading."
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        for h in doc.headings() {
            let last = h.text.chars().last();
            let Some(c) = last else { continue };
            if c != '.' && c != ':' {
                continue;
            }
            let trailing_byte_len = c.len_utf8();
            let local_start = h.text.len().saturating_sub(trailing_byte_len);
            let local_end = h.text.len();
            let message =
                format!("heading ends with `{c}` — headings should not carry sentence punctuation");
            if let Some(d) =
                Diagnostic::at(doc, h.byte_offset, local_start..local_end, message, None)
            {
                out.push(d);
            }
        }
    }
}
