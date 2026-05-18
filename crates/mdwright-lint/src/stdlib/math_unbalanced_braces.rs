//! `math/unbalanced-braces` — `{` / `}` inside a recognised math body
//! do not balance.
//!
//! The math recogniser only checks that opening and closing delimiter
//! tokens (`\[ \]`, `\( \)`, `$ $`, `$$ $$`, `\begin / \end`) match.
//! Brace balance inside the body is a separate invariant: TeX uses
//! `{` / `}` for argument grouping, and an imbalance there means the
//! canonicalise pass cannot safely normalise the body. When the rule
//! fires, body rewrites for that region are skipped, but the underlying
//! typo still needs a human fix.
//!
//! Companion rules [`super::math_unbalanced_delim::MathUnbalancedDelim`]
//! and [`super::math_unbalanced_env::MathUnbalancedEnv`] cover the
//! marker-level imbalances.

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;
use mdwright_document::Document;
use mdwright_document::MathError;

pub struct MathUnbalancedBraces;

impl LintRule for MathUnbalancedBraces {
    fn name(&self) -> &str {
        "math/unbalanced-braces"
    }

    fn description(&self) -> &str {
        "`{` / `}` inside a math body do not balance; math body normalisation is skipped for that region."
    }

    fn explain(&self) -> &str {
        include_str!("explain/math__unbalanced_braces.md")
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for err in doc.math_errors() {
            let MathError::UnbalancedBraces { offset, region } = err else {
                continue;
            };
            let span = (*offset)..offset.saturating_add(1).min(region.end);
            let message = "unbalanced `{` / `}` inside math body — math body normalisation is skipped";
            if let Some(d) = Diagnostic::at(doc, 0, span, message.to_owned(), None) {
                out.push(d);
            }
        }
    }
}
