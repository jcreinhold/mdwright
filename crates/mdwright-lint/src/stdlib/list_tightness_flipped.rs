//! Tightness derived from the structural tree disagrees with tightness
//! derived from the raw source bytes.
//!
//! CM §5.3 makes a list tight iff no item is separated from the next
//! by a blank line and no item contains a direct paragraph child.
//! The tree view reads "direct paragraph child" off parsed list items;
//! the source view reads "blank line between items" off the bytes
//! between consecutive item `raw_range`s. On well-formed input these
//! always agree. Disagreement typically signals an unusual structural
//! shape (e.g., an item whose last child is a fenced or indented code
//! block whose body contains an empty line).
//!
//! Advisory: this is a detector for source/tree disagreement, not a
//! formatter safety gate.

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;
use mdwright_document::Document;
use mdwright_document::ListGroup;

pub struct ListTightnessFlipped;

impl LintRule for ListTightnessFlipped {
    fn name(&self) -> &str {
        "list-tightness-flipped"
    }

    fn description(&self) -> &str {
        "list tightness from the tree disagrees with tightness from source bytes"
    }

    fn explain(&self) -> &str {
        include_str!("explain/list_tightness_flipped.md")
    }

    fn is_default(&self) -> bool {
        false
    }

    fn is_advisory(&self) -> bool {
        true
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        let source = doc.source();
        for (group, tree_tight) in doc.list_tightness_view() {
            let source_tight = source_view_tight(group, source);
            if tree_tight == source_tight {
                continue;
            }
            let message = if tree_tight {
                "list reads as tight from parsed items but source has a blank line between items".to_owned()
            } else {
                "list reads as loose from parsed items but source has no blank line between items".to_owned()
            };
            let start = group.raw_range.start;
            let local = 0..1;
            if let Some(d) = Diagnostic::at(doc, start, local, message, None) {
                out.push(d);
            }
        }
    }
}

/// Source-bytes view of tightness: a list is tight from the source's
/// perspective iff no two consecutive items are separated by a blank
/// line (a line containing only whitespace).
fn source_view_tight(group: &ListGroup, source: &str) -> bool {
    let bytes = source.as_bytes();
    for pair in group.items.windows(2) {
        let [a, b] = pair else { continue };
        let gap_start = a.raw_range.end.min(bytes.len());
        let gap_end = b.raw_range.start.min(bytes.len());
        let Some(gap) = bytes.get(gap_start..gap_end) else {
            continue;
        };
        if has_blank_line(gap) {
            return false;
        }
    }
    true
}

/// `true` iff `bytes` contains a line whose only contents are spaces,
/// tabs, or carriage returns.
fn has_blank_line(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        let line_start = i;
        while bytes.get(i).is_some_and(|b| *b != b'\n') {
            i = i.saturating_add(1);
        }
        let line = bytes.get(line_start..i).unwrap_or(&[]);
        // Skip the very first partial line (everything before the
        // first '\n') — that's the trailing content of the previous
        // item, not a blank gap.
        if line_start > 0 && line.iter().all(|b| matches!(*b, b' ' | b'\t' | b'\r')) {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}
