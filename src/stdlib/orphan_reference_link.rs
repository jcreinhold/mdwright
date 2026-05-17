//! Reference link uses with no matching `[label]:` definition.
//!
//! Detects `[txt][label]` and `[label][]` collapsed forms where the
//! label (case-insensitively) does not appear among the document's
//! link reference definitions. Shortcut form `[label]` is not
//! flagged here — too many false positives on bracketed prose.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;
use crate::util::regex::compile_static;

pub struct OrphanReferenceLink;

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"\[(?P<text>[^\]\n]+)\]\[(?P<label>[^\]\n]*)\]"))
}

impl LintRule for OrphanReferenceLink {
    fn name(&self) -> &str {
        "orphan-reference-link"
    }

    fn description(&self) -> &str {
        "Reference-style link with no matching `[label]:` definition."
    }

    fn explain(&self) -> &str {
        include_str!("explain/orphan_reference_link.md")
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        let defs: HashSet<String> = doc.link_defs().iter().map(|d| d.label.to_ascii_lowercase()).collect();

        for chunk in doc.prose_chunks() {
            for cap in pattern().captures_iter(&chunk.text) {
                let Some(m) = cap.get(0) else { continue };
                let Some(text_match) = cap.name("text") else {
                    continue;
                };
                let Some(label_match) = cap.name("label") else {
                    continue;
                };
                let raw_label = label_match.as_str();
                let key = if raw_label.is_empty() {
                    text_match.as_str().to_ascii_lowercase()
                } else {
                    raw_label.to_ascii_lowercase()
                };
                if defs.contains(&key) {
                    continue;
                }
                let display = if raw_label.is_empty() {
                    text_match.as_str().to_owned()
                } else {
                    raw_label.to_owned()
                };
                let message = format!("reference link `{display}` has no `[{display}]:` definition");
                if let Some(d) = Diagnostic::at(doc, chunk.byte_offset, m.range(), message, None) {
                    out.push(d);
                }
            }
        }
    }
}
