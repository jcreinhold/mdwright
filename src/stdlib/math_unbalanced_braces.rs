//! `math/unbalanced-braces` — `{` / `}` inside a recognised math body
//! do not balance.
//!
//! The math recogniser only checks that opening and closing delimiter
//! tokens (`\[ \]`, `\( \)`, `$ $`, `$$ $$`, `\begin / \end`) match.
//! Brace balance inside the body is a separate invariant: TeX uses
//! `{` / `}` for argument grouping, and an imbalance there means the
//! pretty-printer cannot safely normalise the body (whitespace
//! collapse and ampersand alignment both rely on `{…}` defining
//! self-contained groups). When the rule fires, the pretty-printer
//! falls back to verbatim emission for that region so the document
//! still renders, but the underlying typo needs a human fix.
//!
//! Companion rules [`super::math_unbalanced_delim::MathUnbalancedDelim`]
//! and [`super::math_unbalanced_env::MathUnbalancedEnv`] cover the
//! marker-level imbalances.

use crate::cm::math::span::MathError;
use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct MathUnbalancedBraces;

impl LintRule for MathUnbalancedBraces {
    fn name(&self) -> &str {
        "math/unbalanced-braces"
    }

    fn description(&self) -> &str {
        "`{` / `}` inside a math body do not balance; the pretty-printer falls back to verbatim emission for that region."
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        for err in doc.math_errors() {
            let MathError::UnbalancedBraces { offset, region } = err else {
                continue;
            };
            let span = (*offset)..offset.saturating_add(1).min(region.end);
            let message =
                "unbalanced `{` / `}` inside math body — pretty-printer emits this region verbatim";
            if let Some(d) = Diagnostic::at(doc, 0, span, message.to_owned(), None) {
                out.push(d);
            }
        }
    }
}
