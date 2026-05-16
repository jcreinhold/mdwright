//! `math/unbalanced-delim` — TeX-style math opener with no matching
//! close.
//!
//! Mdwright recognises `\[ … \]`, `\( … \)`, `$$ … $$`, and `$ … $`.
//! An open delimiter with no matching close is almost always a typo
//! or a copy-paste accident: the rest of the document collapses into
//! "math content" in the author's mental model, but pulldown-cmark
//! parses it as prose and the document renders badly.
//!
//! Companion rule [`super::math_unbalanced_env::MathUnbalancedEnv`]
//! covers `\begin{env}` / `\end{env}` imbalance.

use crate::cm::math::span::MathError;
use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct MathUnbalancedDelim;

impl LintRule for MathUnbalancedDelim {
    fn name(&self) -> &str {
        "math/unbalanced-delim"
    }

    fn description(&self) -> &str {
        "TeX-style math open delimiter (`\\[`, `\\(`, `$$`, `$`) with no matching close."
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        for err in doc.math_errors() {
            let MathError::UnbalancedDelim { delim, range } = err else {
                continue;
            };
            let open_lit = delim.open();
            let close_lit = delim.close();
            let flavour = if delim.is_display() {
                "display-math"
            } else {
                "inline-math"
            };
            let message = format!(
                "unbalanced {flavour} `{open_lit}` — no matching `{close_lit}` before end of document or next code/HTML block"
            );
            if let Some(d) = Diagnostic::at(doc, 0, range.clone(), message, None) {
                out.push(d);
            }
        }
    }
}
