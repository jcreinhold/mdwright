//! `math/unbalanced-env` — LaTeX `\begin{env}` with no matching
//! `\end{env}` at the same nesting depth.
//!
//! Environments outside `\[ … \]` are common in mathematical prose
//! (`KaTeX` renders them directly). An open `\begin` with no close
//! turns the rest of the document into math in the author's mental
//! model; pulldown-cmark parses it as prose and the document
//! renders badly.
//!
//! Companion rule [`super::math_unbalanced_delim::MathUnbalancedDelim`]
//! covers primitive delimiter imbalance.

use crate::cm::math::span::MathError;
use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct MathUnbalancedEnv;

impl LintRule for MathUnbalancedEnv {
    fn name(&self) -> &str {
        "math/unbalanced-env"
    }

    fn description(&self) -> &str {
        "LaTeX `\\begin{env}` with no matching `\\end{env}` at the same nesting depth."
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        for err in doc.math_errors() {
            let MathError::UnbalancedEnv { name, range } = err else {
                continue;
            };
            let message = format!(
                "unbalanced `\\begin{{{name}}}` — no matching `\\end{{{name}}}` before end of document or next code/HTML block"
            );
            if let Some(d) = Diagnostic::at(doc, 0, range.clone(), message, None) {
                out.push(d);
            }
        }
    }
}
