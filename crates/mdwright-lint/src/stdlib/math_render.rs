//! `math/render-compat` — math-renderer compatibility checks for every math
//! span in the document.
//!
//! Names like MathJax and KaTeX appear unbacktickd in prose; the
//! `clippy::doc_markdown` allow on the module covers that.
//!
//! Umbrella rule that emits diagnostics under several per-kind rule codes:
//!
//! - `math/render-unsupported-command`
//! - `math/render-missing-package`
//! - `math/render-unsupported-environment`
//! - `math/render-missing-package-env`
//! - `math/render-math-command-in-text`
//!
//! The umbrella name `math/render-compat` is what users disable to turn the
//! whole family off; the per-kind names are what users disable to silence one
//! kind only. The dispatcher in `rule_set.rs` preserves the per-kind codes set
//! by this rule.
//!
//! The active renderer (MathJax v3, KaTeX) and its loaded packages come from
//! `[lint.render]` in the project config; the lint rule itself only carries
//! the resolved profile.

#![allow(
    clippy::doc_markdown,
    reason = "names like MathJax and KaTeX appear in prose; backticking each one would add noise"
)]

use std::borrow::Cow;

use mdwright_document::{Document, MathBody};
use mdwright_mathrender::{RenderIssue, RenderProfile, check_math_body};

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;

/// Math-renderer compatibility lint. Construct with `new()` for the
/// MathJax v3 default profile; the CLI swaps in a config-derived profile via
/// `with_profile`.
pub struct RenderCompat {
    profile: RenderProfile,
}

impl RenderCompat {
    #[must_use]
    pub fn new() -> Self {
        Self {
            profile: RenderProfile::mathjax_v3(),
        }
    }

    #[must_use]
    pub fn with_profile(profile: RenderProfile) -> Self {
        Self { profile }
    }
}

impl Default for RenderCompat {
    fn default() -> Self {
        Self::new()
    }
}

impl LintRule for RenderCompat {
    fn name(&self) -> &str {
        "math/render-compat"
    }

    fn description(&self) -> &str {
        "Math-renderer compatibility for inline and display math (MathJax v3 / KaTeX)."
    }

    fn is_default(&self) -> bool {
        false
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for region in doc.math_regions() {
            let body = region.span().body();
            let cleaned = body.as_str(doc.source());
            for issue in check_math_body(cleaned.as_ref(), &self.profile) {
                if let Some(diagnostic) = into_diagnostic(doc, body, &self.profile, &issue) {
                    out.push(diagnostic);
                }
            }
        }
    }
}

fn into_diagnostic(
    doc: &Document,
    body: &MathBody,
    profile: &RenderProfile,
    issue: &RenderIssue,
) -> Option<Diagnostic> {
    let renderer = profile.renderer().name();
    let pkg_noun = profile.renderer().package_noun();
    let (code, message, span) = match issue {
        RenderIssue::UnsupportedCommand { name, span } => (
            "math/render-unsupported-command",
            format!("{renderer} does not ship a command `\\{name}` in any {pkg_noun}."),
            span,
        ),
        RenderIssue::MissingPackage { name, package, span } => (
            "math/render-missing-package",
            format!("command `\\{name}` requires the {renderer} `{package}` {pkg_noun}, which is not loaded."),
            span,
        ),
        RenderIssue::UnsupportedEnvironment { name, span } => (
            "math/render-unsupported-environment",
            format!("{renderer} does not ship an environment `{name}` in any {pkg_noun}."),
            span,
        ),
        RenderIssue::MissingPackageEnv { name, package, span } => (
            "math/render-missing-package-env",
            format!("environment `{name}` requires the {renderer} `{package}` {pkg_noun}, which is not loaded."),
            span,
        ),
        RenderIssue::MathCommandInTextMode { name, span } => (
            "math/render-math-command-in-text",
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
        run_with(src, &RenderCompat::new())
    }

    fn run_with(src: &str, rule: &RenderCompat) -> Vec<Diagnostic> {
        let opts = ParseOptions::default().with_math(MathParseOptions {
            delimiters: MathDelimiterSet::Github,
        });
        let doc = Document::parse_with_options(src, opts).expect("parse");
        let mut out = Vec::new();
        rule.check(&doc, &mut out);
        out
    }

    #[test]
    fn flags_chemistry_command_without_mhchem_default_mathjax() {
        let diagnostics = run(r"Reaction: $\ce{H2O}$ here.");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_ref()).collect();
        assert_eq!(codes, vec!["math/render-missing-package"]);
        let first = diagnostics.first().expect("at least one diagnostic");
        assert!(first.message.contains("MathJax v3"));
    }

    #[test]
    fn diagnostic_message_names_the_active_renderer_for_katex() {
        let rule = &RenderCompat::with_profile(RenderProfile::katex());
        let diagnostics = run_with(r"Reaction: $\ce{H2O}$ here.", rule);
        let first = diagnostics.first().expect("at least one diagnostic");
        assert_eq!(diagnostics.len(), 1);
        assert!(first.message.contains("KaTeX"));
        assert!(first.message.contains("extension"));
    }

    #[test]
    fn flags_unsupported_environment() {
        let diagnostics = run("$$\n\\begin{tikzpicture}x\\end{tikzpicture}\n$$\n");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_ref()).collect();
        assert_eq!(codes, vec!["math/render-unsupported-environment"]);
    }

    #[test]
    fn flags_math_in_text_mode() {
        let diagnostics = run(r"Inline: $\text{value is \alpha}$ here.");
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_ref()).collect();
        assert_eq!(codes, vec!["math/render-math-command-in-text"]);
    }

    #[test]
    fn ignores_well_formed_math_under_mathjax_default() {
        let diagnostics = run(r"Hello: $\frac{a}{b} + \sqrt{x}$.");
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn ignores_well_formed_math_under_katex() {
        let rule = &RenderCompat::with_profile(RenderProfile::katex());
        let diagnostics = run_with(r"Hello: $\mathbb{R} \xrightarrow{f} \mathfrak{m}$.", rule);
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
        let rule = &RenderCompat::with_profile(RenderProfile::mathjax_v3().with_package("mhchem"));
        let diagnostics = run_with(r"Reaction: $\ce{H2O}$.", rule);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
