//! Curated public facade for mdwright.
//!
//! The implementation lives in focused crates: document recognition,
//! formatting, linting, configuration, and delivery. This crate keeps
//! the common user-facing imports in one place without re-exporting the
//! full internal module trees.
//!
//! ```
//! use mdwright::{Document, RuleSet};
//!
//! let src = "## A heading.\n\nA bare URL: https://example.com\n";
//! let doc = Document::parse(src);
//! let diags = RuleSet::stdlib_defaults().check(&doc);
//! assert!(diags.iter().any(|d| d.rule == "bare-url"));
//! ```

#![forbid(unsafe_code)]

pub use mdwright_config::{Config, ConfigError};
pub use mdwright_document::{
    AllowScope, CodeBlock, Document, ExtensionOptions, Frontmatter, FrontmatterDelimiter, Heading, HtmlBlock,
    InlineCode, InlineHtml, LinkDef, ListGroup, ListItem, MathError, MathRegion, MathSpan, MystOptions, PandocOptions,
    ParseOptions, Suppression, SuppressionKind, TextSlice, contains_rejected_control_chars,
};
pub use mdwright_format::{
    CheckpointTable, EndOfLine, FmtOptions, FormatError, HeadingAttrsStyle, ItalicStyle, LinkDefStyle, ListMarkerStyle,
    MathOptions, MathRender, OrderedListStyle, Placement, StrongStyle, ThematicStyle, TrailingNewline, Wrap,
    first_divergence, format_document, format_range, format_range_with_checkpoints, format_source, format_validated,
    semantically_equivalent,
};
pub use mdwright_lint::{
    DOCS_URL_DEFAULT, Diagnostic, DuplicateRuleName, Fix, LintOptions, LintRule, RuleSet, Severity, Snippet,
    apply_safe_fixes, docs_url, rule_doc_url,
};

/// Standard lint rules.
pub mod stdlib {
    pub use mdwright_lint::stdlib::{all, by_name, defaults, names};
}

#[cfg(test)]
mod tests {
    use super::{Document, RuleSet, contains_rejected_control_chars};

    #[test]
    fn control_char_predicate_accepts_clean_text() {
        assert!(!contains_rejected_control_chars(""));
        assert!(!contains_rejected_control_chars("# hello\n\nworld\n"));
        assert!(!contains_rejected_control_chars("tab\there\tand\nlf\n"));
        assert!(!contains_rejected_control_chars("ff:\x0c, cr:\r\n"));
        assert!(!contains_rejected_control_chars("del:\x7f"));
    }

    #[test]
    fn control_char_predicate_rejects_c0_controls() {
        assert!(contains_rejected_control_chars("nul:\0"));
        assert!(contains_rejected_control_chars("eot:\x04"));
        assert!(contains_rejected_control_chars("vt:\x0b"));
        assert!(contains_rejected_control_chars("so:\x0e"));
        assert!(contains_rejected_control_chars("us:\x1f"));
    }

    #[test]
    fn facade_lint_smoke() {
        let doc = Document::parse("See https://example.com for details.\n");
        let diags = RuleSet::stdlib_defaults().check(&doc);
        assert!(diags.iter().any(|d| d.rule == "bare-url"));
    }
}
