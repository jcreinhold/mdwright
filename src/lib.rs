//! Math-resilient Markdown linter with a public, extensible rule
//! trait.
//!
//! ## Design
//!
//! [`Document`] parses a Markdown source once and exposes a curated
//! query surface (`prose_chunks`, `inline_codes`, `headings`, …) over
//! the result. Rule authors implement [`LintRule`] and register their
//! rule with a [`RuleSet`]; running `doc.lint(&rules)` returns a
//! sorted list of [`Diagnostic`]s.
//!
//! The crate ships a standard library of fifteen rules under
//! [`stdlib`]; [`RuleSet::stdlib_defaults`] returns the curated
//! default-on subset and [`RuleSet::stdlib_all`] returns the lot
//! (including opt-in checks for legacy `mdformat`-damage detection
//! and the project's no-LaTeX-in-prose convention).
//!
//! ## Quick start
//!
//! ```
//! use mdwright::{Document, RuleSet};
//!
//! let src = "## A heading.\n\nA bare URL: https://example.com\n";
//! let doc = Document::parse(src);
//! let diags = doc.lint(&RuleSet::stdlib_defaults());
//! assert!(diags.iter().any(|d| d.rule == "heading-punctuation"));
//! assert!(diags.iter().any(|d| d.rule == "bare-url"));
//! ```
//!
//! ## Extending with your own rule
//!
//! ```
//! use mdwright::{Diagnostic, Document, LintRule, RuleSet};
//!
//! struct NoBrTag;
//! impl LintRule for NoBrTag {
//!     fn name(&self) -> &str { "no-br-tag" }
//!     fn description(&self) -> &str { "Disallow <br> tags in prose." }
//!     fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
//!         for h in doc.inline_html() {
//!             if h.text.eq_ignore_ascii_case("<br>") {
//!                 if let Some(d) = Diagnostic::at(
//!                     doc, h.byte_offset, 0..h.text.len(),
//!                     "use a blank line, not <br>".to_owned(), None,
//!                 ) { out.push(d); }
//!             }
//!         }
//!     }
//! }
//!
//! let mut rs = RuleSet::stdlib_defaults();
//! rs.add(Box::new(NoBrTag)).expect("unique name");
//! let _ = Document::parse("hello<br>world").lint(&rs);
//! ```

mod cm;
mod config;
mod diagnostic;
mod discover;
mod document;
mod format;
mod ir;
mod line_index;
mod rule;
mod rule_set;
mod source;
pub mod stdlib;
mod suppression;
mod tree;
mod util;

pub use config::{
    Config, ConfigError, EndOfLine, FmtOptions, FormatMode, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions,
    OrderedListStyle, Placement, TrailingNewline, Wrap,
};
pub use diagnostic::{Diagnostic, Fix};
pub use discover::discover_markdown;
pub use document::{Document, FormatError, LintOptions, render_html};
pub use ir::{
    AllowScope, CodeBlock, Frontmatter, FrontmatterDelimiter, Heading, HtmlBlock, InlineCode, InlineHtml, LinkDef,
    ListGroup, ListItem, Suppression, SuppressionKind, TextSlice,
};
pub use line_index::LineIndex;
pub use rule::LintRule;
pub use rule_set::{DuplicateRuleName, RuleSet};

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Document, LintRule, RuleSet};

    fn diags(src: &str) -> Vec<Diagnostic> {
        Document::parse(src).lint(&RuleSet::stdlib_all())
    }

    #[test]
    fn detects_escaped_emphasis_under_all() {
        let d = diags(r"This is \_broken\_ italic.");
        assert!(d.iter().any(|d| d.rule == "escaped-emphasis"));
    }

    #[test]
    fn defaults_skip_opt_in_rules() {
        let src = r"This is \_broken\_ italic and $ in prose.";
        let d = Document::parse(src).lint(&RuleSet::stdlib_defaults());
        assert!(!d.iter().any(|d| d.rule == "escaped-emphasis"));
        assert!(!d.iter().any(|d| d.rule == "stray-dollar"));
    }

    #[test]
    fn detects_adjacent_code() {
        let d = diags("see `foo`bar nearby");
        assert!(d.iter().any(|d| d.rule == "adjacent-code-no-space"));
    }

    #[test]
    fn detects_unbalanced_backtick() {
        let d = diags("a `foo and more\nnot closing\n");
        assert!(d.iter().any(|d| d.rule == "unbalanced-backtick"));
    }

    #[test]
    fn fenced_code_is_skipped() {
        let src = "before\n```\n\\_inside\\_\n```\nafter \\_outside\\_\n";
        let d = diags(src);
        let outside = d.iter().filter(|d| d.rule == "escaped-emphasis").count();
        assert_eq!(outside, 2);
    }

    #[test]
    fn rule_set_only_named() -> anyhow::Result<()> {
        let mut rs = RuleSet::new();
        let rule = super::stdlib::by_name("stray-dollar")
            .ok_or_else(|| anyhow::anyhow!("stray-dollar should be a stdlib rule"))?;
        rs.add(rule).map_err(|e| anyhow::anyhow!("{e}"))?;
        let d = Document::parse("a $ and \\_b\\_ here").lint(&rs);
        assert!(d.iter().all(|d| d.rule == "stray-dollar"));
        Ok(())
    }

    #[test]
    fn detects_heading_punctuation() {
        let d = diags("## Heading.\n");
        assert!(d.iter().any(|d| d.rule == "heading-punctuation"));
    }

    #[test]
    fn detects_bare_url() {
        let d = diags("See https://example.com for details.\n");
        assert!(d.iter().any(|d| d.rule == "bare-url"));
    }

    #[test]
    fn extensibility_smoke() -> anyhow::Result<()> {
        struct Counter;
        impl LintRule for Counter {
            fn name(&self) -> &str {
                "user-counter"
            }
            fn description(&self) -> &str {
                ""
            }
            fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
                if doc.prose_chunks().iter().any(|c| c.text.contains("foo"))
                    && let Some(d) = Diagnostic::at(doc, 0, 0..1, "found foo".to_owned(), None)
                {
                    out.push(d);
                }
            }
        }
        let mut rs = RuleSet::new();
        rs.add(Box::new(Counter)).map_err(|e| anyhow::anyhow!("{e}"))?;
        let d = Document::parse("hello foo world").lint(&rs);
        assert!(d.iter().any(|d| d.rule == "user-counter"));
        Ok(())
    }
}
