//! `math/mathjax-compat` — `MathJax` v3 compatibility checks for every math
//! span in the document.
//!
//! This is an umbrella rule that emits diagnostics under several rule codes,
//! one per kind of incompatibility:
//!
//! - `math/mathjax-unsupported-command`
//! - `math/mathjax-missing-package`
//! - `math/mathjax-unsupported-environment`
//! - `math/mathjax-missing-package-env`
//! - `math/mathjax-math-command-in-text`
//!
//! The umbrella name `math/mathjax-compat` is what users disable to turn the
//! whole family off; the per-kind names are what users disable to silence one
//! kind only. The dispatcher in `rule_set.rs` preserves the per-kind codes set
//! by this rule.

use std::borrow::Cow;

use mdwright_document::{Document, MathBody};
use mdwright_mathjax::{MathJaxIssue, MathJaxProfile, check_math_body};

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;

/// `MathJax` v3 compatibility lint. Construct with `new()` for the default
/// profile (`v3_default()`); the CLI swaps in a config-derived profile via
/// `with_profile`.
pub struct MathJaxCompat {
    profile: MathJaxProfile,
}

impl MathJaxCompat {
    #[must_use]
    pub fn new() -> Self {
        Self {
            profile: MathJaxProfile::v3_default(),
        }
    }

    #[must_use]
    pub fn with_profile(profile: MathJaxProfile) -> Self {
        Self { profile }
    }
}

impl Default for MathJaxCompat {
    fn default() -> Self {
        Self::new()
    }
}

impl LintRule for MathJaxCompat {
    fn name(&self) -> &str {
        "math/mathjax-compat"
    }

    fn description(&self) -> &str {
        "MathJax v3 compatibility for inline and display math."
    }

    fn is_default(&self) -> bool {
        false
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for region in doc.math_regions() {
            let body = region.span().body();
            let cleaned = body.as_str(doc.source());
            for issue in check_math_body(cleaned.as_ref(), &self.profile) {
                if let Some(diagnostic) = into_diagnostic(doc, body, &issue) {
                    out.push(diagnostic);
                }
            }
        }
    }
}

fn into_diagnostic(doc: &Document, body: &MathBody, issue: &MathJaxIssue) -> Option<Diagnostic> {
    let (code, message, span) = match issue {
        MathJaxIssue::UnsupportedCommand { name, span } => (
            "math/mathjax-unsupported-command",
            format!("MathJax does not ship a command `\\{name}` in any package."),
            span,
        ),
        MathJaxIssue::MissingPackage { name, package, span } => (
            "math/mathjax-missing-package",
            format!("command `\\{name}` requires the MathJax `{package}` package, which is not loaded."),
            span,
        ),
        MathJaxIssue::UnsupportedEnvironment { name, span } => (
            "math/mathjax-unsupported-environment",
            format!("MathJax does not ship an environment `{name}` in any package."),
            span,
        ),
        MathJaxIssue::MissingPackageEnv { name, package, span } => (
            "math/mathjax-missing-package-env",
            format!("environment `{name}` requires the MathJax `{package}` package, which is not loaded."),
            span,
        ),
        MathJaxIssue::MathCommandInTextMode { name, span } => (
            "math/mathjax-math-command-in-text",
            format!("math-mode command `\\{name}` inside `\\text{{...}}` will render as plain text, not as math."),
            span,
        ),
    };
    let range = span.as_range();
    let start = body.clean_offset_to_source(range.start);
    let end = body.clean_offset_to_source(range.end);
    let mut diagnostic = Diagnostic::at(doc, 0, start..end, message, None)?;
    diagnostic.rule = Cow::Borrowed(code);
    Some(diagnostic)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests assert diagnostic shape against fixed inputs")]

    use super::*;
    use mdwright_document::{Document, MathDelimiterSet, MathParseOptions, ParseOptions};

    fn run(src: &str) -> Vec<Diagnostic> {
        let opts = ParseOptions::default().with_math(MathParseOptions {
            delimiters: MathDelimiterSet::Github,
        });
        let doc = Document::parse_with_options(src, opts).expect("parse");
        let mut out = Vec::new();
        MathJaxCompat::new().check(&doc, &mut out);
        out
    }

    #[test]
    fn flags_chemistry_command_without_mhchem() {
        let diagnostics = run(r"Reaction: $\ce{H2O}$ here.");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_ref()).collect();
        assert_eq!(codes, vec!["math/mathjax-missing-package"]);
    }

    #[test]
    fn flags_unsupported_environment() {
        let diagnostics = run("$$\n\\begin{tikzpicture}x\\end{tikzpicture}\n$$\n");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_ref()).collect();
        assert_eq!(codes, vec!["math/mathjax-unsupported-environment"]);
    }

    #[test]
    fn flags_math_in_text_mode() {
        let diagnostics = run(r"Inline: $\text{value is \alpha}$ here.");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_ref()).collect();
        assert_eq!(codes, vec!["math/mathjax-math-command-in-text"]);
    }

    #[test]
    fn ignores_well_formed_math() {
        let diagnostics = run(r"Hello: $\frac{a}{b} + \sqrt{x}$.");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn spans_resolve_to_source_positions() {
        let src = r"Hello: $\ce{H2O}$.";
        let diagnostics = run(src);
        let diagnostic = diagnostics.first().expect("missing diagnostic");
        let captured = src.get(diagnostic.span.clone()).expect("span");
        assert_eq!(captured, r"\ce");
    }

    #[test]
    fn loading_package_via_profile_silences_diagnostic() {
        let src = r"Reaction: $\ce{H2O}$.";
        let opts = ParseOptions::default().with_math(MathParseOptions {
            delimiters: MathDelimiterSet::Github,
        });
        let doc = Document::parse_with_options(src, opts).expect("parse");
        let mut out = Vec::new();
        let rule = MathJaxCompat::with_profile(MathJaxProfile::v3_default().with_package("mhchem"));
        rule.check(&doc, &mut out);
        assert!(out.is_empty(), "{out:?}");
    }
}
