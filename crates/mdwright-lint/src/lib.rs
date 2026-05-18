#![forbid(unsafe_code)]

mod diagnostic;
mod regex_util;
mod rule;
mod rule_set;
pub mod stdlib;
mod suppression;

use std::ops::Range;

use mdwright_document::{ByteSpan, Document};

pub use diagnostic::{DOCS_URL_DEFAULT, Diagnostic, Fix, Severity, Snippet, docs_url, rule_doc_url};
pub use rule::LintRule;
pub use rule_set::{DuplicateRuleName, RuleSet};

/// Knobs that change lint execution.
#[derive(Copy, Clone, Debug)]
pub struct LintOptions {
    /// Whether `<!-- mdwright: allow ... -->` comments filter diagnostics.
    pub respect_suppressions: bool,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            respect_suppressions: true,
        }
    }
}

/// Apply every safe fix from `diags` to the document's original source.
#[must_use]
pub fn apply_safe_fixes(doc: &Document, diags: &[Diagnostic]) -> (String, usize) {
    let mut edits: Vec<(Range<usize>, &str)> = diags
        .iter()
        .filter_map(|d| {
            let fix = d.fix.as_ref().filter(|f| f.safe)?;
            let canon = ByteSpan::new(u32::try_from(d.span.start).ok()?, u32::try_from(d.span.end).ok()?);
            let orig = doc.source_handle().to_original(canon);
            Some((orig.range(), fix.replacement.as_str()))
        })
        .collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut out = doc.source_handle().original().to_owned();
    let mut applied = 0usize;
    let mut last_start = usize::MAX;
    for (range, replacement) in edits {
        if range.end > last_start {
            continue;
        }
        out.replace_range(range.clone(), replacement);
        last_start = range.start;
        applied = applied.saturating_add(1);
    }
    (out, applied)
}
