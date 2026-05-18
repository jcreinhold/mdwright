//! Stray `$` in prose.
//!
//! Opinionated rule: some projects (this one included) use Unicode
//! math inside inline code spans rather than `$…$` LaTeX delimiters,
//! so a literal `$` in prose is usually a typo. Default-off because
//! other projects rely on `$…$` for math.

use crate::diagnostic::Diagnostic;
use crate::diagnostic::Fix;
use crate::rule::LintRule;
use mdwright_document::Document;

pub struct StrayDollar;

impl LintRule for StrayDollar {
    fn name(&self) -> &str {
        "stray-dollar"
    }

    fn description(&self) -> &str {
        "Literal `$` in prose (opt-in for projects that don't use $…$ math)."
    }

    fn explain(&self) -> &str {
        include_str!("explain/stray_dollar.md")
    }

    fn produces_fix(&self) -> bool {
        true
    }

    fn is_default(&self) -> bool {
        false
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for chunk in doc.prose_chunks() {
            for (idx, _) in chunk.text.match_indices('$') {
                let message = "stray `$` — this project uses Unicode math in inline code \
                     spans, not LaTeX delimiters"
                    .to_owned();
                if let Some(d) = Diagnostic::at(
                    doc,
                    chunk.byte_offset,
                    idx..idx.saturating_add(1),
                    message,
                    Some(Fix {
                        replacement: "`".to_owned(),
                        safe: false,
                    }),
                ) {
                    out.push(d);
                }
            }
        }
    }
}
