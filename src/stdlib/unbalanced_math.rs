//! A TeX-style math open delimiter with no matching close.
//!
//! `mdwright` recognises four math-delimiter pairs (`\[ … \]`,
//! `\( … \)`, and optionally `$$ … $$` / `$ … $`). An open with no
//! matching close is almost always a typo or a copy-paste accident:
//! the rest of the document collapses into "math content" in the
//! author's mental model, but `pulldown-cmark` parses it as prose
//! and the document renders badly.
//!
//! The scanner that produces the formatter's math-region overlay
//! also records every unbalanced open it sees; this rule surfaces
//! them as diagnostics with a span on the open delimiter itself.
//! No fix is offered: the right repair is human judgement (insert
//! the close, escape the open, or rewrite as inline math).

use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct UnbalancedMath;

impl LintRule for UnbalancedMath {
    fn name(&self) -> &str {
        "unbalanced-math-delim"
    }

    fn description(&self) -> &str {
        "TeX-style math open delimiter (`\\[`, `\\(`, `$$`, `$`) with no matching close."
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        for unclosed in doc.unclosed_math() {
            let open_lit = unclosed.delim.open();
            let close_lit = unclosed.delim.close();
            let flavour = if unclosed.delim.is_display() {
                "display-math"
            } else {
                "inline-math"
            };
            let message = format!(
                "unbalanced {flavour} `{open_lit}` — no matching `{close_lit}` before end of document or next code/HTML block"
            );
            if let Some(d) = Diagnostic::at(doc, 0, unclosed.range.clone(), message, None) {
                out.push(d);
            }
        }
    }
}
