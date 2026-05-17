//! Two headings at the same level under the same parent with
//! identical trimmed text.
//!
//! A table of contents collapses duplicate entries silently; anchor
//! links resolve to the first occurrence only. Both outcomes are
//! usually wrong. Parent is taken to be the most recent
//! strictly-shallower heading; under no parent (top level), siblings
//! are all top-level headings of the same level.

use std::collections::HashMap;

use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct DuplicateHeading;

impl LintRule for DuplicateHeading {
    fn name(&self) -> &str {
        "duplicate-heading"
    }

    fn description(&self) -> &str {
        "Two headings at the same level under the same parent with the same text."
    }

    fn is_advisory(&self) -> bool {
        // Math / theorem documents legitimately repeat `### Proof`,
        // `### Corollary`, etc. under one chapter heading. Useful
        // signal for prose; noisy for math. Advisory by default so
        // it still surfaces but doesn't fail `--check`.
        true
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        // `path[i]` is the currently-open heading at level (i+1), or
        // None if no heading at that level is currently open. When a
        // heading at level L is seen, levels >= L are closed.
        let mut path: [Option<String>; 6] = Default::default();
        let mut seen: HashMap<(usize, String, String), usize> = HashMap::new();
        for h in doc.headings() {
            let level = h.level as usize;
            // Parent = closest non-None slot strictly above this level.
            let parent_key = (0..level.saturating_sub(1))
                .rev()
                .find_map(|i| path.get(i).and_then(Option::as_ref).cloned())
                .unwrap_or_default();
            let key = (level, parent_key, h.text.to_ascii_lowercase());
            if let Some(&first_offset) = seen.get(&key) {
                let message = format!(
                    "duplicate heading `{}` at level {level}; first defined at byte {first_offset}",
                    h.text
                );
                let local = 0..(h.raw_range.end.saturating_sub(h.raw_range.start));
                if let Some(d) = Diagnostic::at(doc, h.raw_range.start, local, message, None) {
                    out.push(d);
                }
            } else {
                seen.insert(key, h.raw_range.start);
            }
            // Open this heading at its level; close deeper levels.
            if let Some(slot) = path.get_mut(level.saturating_sub(1)) {
                *slot = Some(h.text.to_owned());
            }
            for i in level..path.len() {
                if let Some(slot) = path.get_mut(i) {
                    *slot = None;
                }
            }
        }
    }
}
