//! A backtick in prose that pulldown-cmark could not pair.
//!
//! `CommonMark`'s parser pairs backtick runs greedily. If a literal
//! `` ` `` survives into a prose chunk, no matching closing fence
//! was found and the inline code span did not close. Renderers that
//! treat the unmatched run as prose tend to mangle nearby `_` or `*`
//! — flagging this early prevents that.

use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct UnbalancedBacktick;

impl LintRule for UnbalancedBacktick {
    fn name(&self) -> &str {
        "unbalanced-backtick"
    }

    fn description(&self) -> &str {
        "Backtick in prose that could not be paired with a closing fence."
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        for chunk in doc.prose_chunks() {
            for (idx, _) in chunk.text.match_indices('`') {
                let message = "unclosed inline code span — pulldown-cmark could not pair \
                     this backtick with a closing fence"
                    .to_owned();
                if let Some(d) = Diagnostic::at(
                    doc,
                    chunk.byte_offset,
                    idx..idx.saturating_add(1),
                    message,
                    None,
                ) {
                    out.push(d);
                }
            }
        }
    }
}
