//! Two `[label]:` definitions with the same case-insensitive label.
//!
//! `CommonMark` resolves duplicate labels to the first definition,
//! silently ignoring subsequent ones — easy to introduce by accident,
//! easy to miss in review. Each duplicate after the first is flagged.

use std::collections::HashMap;

use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct DuplicateLinkLabel;

impl LintRule for DuplicateLinkLabel {
    fn name(&self) -> &str {
        "duplicate-link-label"
    }

    fn description(&self) -> &str {
        "Two `[label]:` definitions with the same label."
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for def in doc.link_defs() {
            let key = def.label.to_ascii_lowercase();
            match seen.get(&key) {
                Some(&first_line_byte) => {
                    let message = format!(
                        "duplicate link reference `{}` — first defined at byte {first_line_byte}",
                        def.label
                    );
                    let local = 0..(def.raw_range.end.saturating_sub(def.raw_range.start));
                    if let Some(d) = Diagnostic::at(doc, def.raw_range.start, local, message, None) {
                        out.push(d);
                    }
                }
                None => {
                    seen.insert(key, def.raw_range.start);
                }
            }
        }
    }
}
