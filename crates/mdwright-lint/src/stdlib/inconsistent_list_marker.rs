//! Mixed `-` / `*` / `+` markers within one bullet list.
//!
//! Pick one and stick to it. Mixed markers look like a stale merge
//! and confuse renderers that re-emit the list with a normalised
//! marker. Ordered lists are not flagged — their markers are digits
//! and have semantic meaning.

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;
use mdwright_document::Document;

pub struct InconsistentListMarker;

impl LintRule for InconsistentListMarker {
    fn name(&self) -> &str {
        "inconsistent-list-marker"
    }

    fn description(&self) -> &str {
        "Mixed `-` / `*` / `+` markers in one bullet list."
    }

    fn explain(&self) -> &str {
        include_str!("explain/inconsistent_list_marker.md")
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for group in doc.list_groups() {
            if group.ordered || group.items.is_empty() {
                continue;
            }
            let Some(first) = group.items.first() else {
                continue;
            };
            let expected = first.marker_byte;
            for item in group.items.iter().skip(1) {
                if item.marker_byte != expected {
                    let actual = item.marker_byte as char;
                    let expected_c = expected as char;
                    let message = format!(
                        "list marker `{actual}` does not match first item's `{expected_c}` \
                         — keep one marker per list"
                    );
                    let start = item.raw_range.start;
                    let local = 0..1;
                    if let Some(d) = Diagnostic::at(doc, start, local, message, None) {
                        out.push(d);
                    }
                }
            }
        }
    }
}
