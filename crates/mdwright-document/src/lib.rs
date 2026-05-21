#![forbid(unsafe_code)]

mod document;
mod error;
mod format_facts;
mod gfm;
mod heading;
mod ir;
mod line_index;
mod parse;
mod refs;
mod render;
mod signature;
mod source;
mod tree;
mod util;

pub use document::{Document, render_html, render_html_with_options, render_html_with_render_options};
pub use error::ParseError;
pub use format_facts::{
    HeadingAttrSite, InlineDelimiterKind, InlineDelimiterSlot, InlineLinkDestinationSlot, OrderedListMarkerSite,
    ParagraphHardBreak, ReferenceDefinitionSite, StructuralKind, StructuralSpan, TableCellSite, TableRowSite,
    TableSite, UnorderedListMarkerSite, WrappableParagraph,
};
pub use gfm::{AutolinkFact, AutolinkOrigin};
pub use heading::HeadingAttrs;
pub use ir::{
    AllowScope, BlockCheckpointFact, CodeBlock, Frontmatter, FrontmatterDelimiter, Heading, HtmlBlock, InlineCode,
    InlineHtml, LinkDef, ListGroup, ListItem, Suppression, SuppressionKind, TextSlice,
};
pub use line_index::{LineIndex, LineIndexError};
pub use mdwright_math::{MathError, MathRegion, MathSpan};
pub use render::{RenderOptions, RenderProfile};
pub use signature::{MarkdownSignature, markdown_signature};
pub use tree::TableAlign;

/// Markdown recognition policy.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct ParseOptions {
    extensions: ExtensionOptions,
    math: MathParseOptions,
}

impl ParseOptions {
    /// Extension-recognition toggles.
    #[must_use]
    pub fn extensions(&self) -> ExtensionOptions {
        self.extensions
    }

    /// Math-source recognition policy.
    #[must_use]
    pub fn math(&self) -> MathParseOptions {
        self.math
    }

    /// Override extension-recognition toggles.
    #[must_use]
    pub fn with_extensions(mut self, extensions: ExtensionOptions) -> Self {
        self.extensions = extensions;
        self
    }

    /// Override math-source recognition policy.
    #[must_use]
    pub fn with_math(mut self, math: MathParseOptions) -> Self {
        self.math = math;
        self
    }
}

/// Math delimiter recognition policy.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MathParseOptions {
    pub delimiters: MathDelimiterSet,
}

impl Default for MathParseOptions {
    fn default() -> Self {
        Self {
            delimiters: MathDelimiterSet::Tex,
        }
    }
}

impl MathParseOptions {
    pub(crate) fn scanner_config(self) -> mdwright_math::MathConfig {
        let mut cfg = mdwright_math::MathConfig::default();
        match self.delimiters {
            MathDelimiterSet::Tex => {}
            MathDelimiterSet::Github => {
                cfg.double_dollar = true;
                cfg.single_dollar = true;
            }
        }
        cfg
    }
}

/// Named math delimiter sets recognised by the Markdown parser.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum MathDelimiterSet {
    /// TeX delimiters: `\(...\)`, `\[...\]`, and LaTeX environments.
    #[default]
    Tex,
    /// GitHub-style dollar math, plus the TeX delimiters.
    Github,
}

/// Per-extension recognition toggles.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one toggle per mdformat-mkdocs extension; the parallel naming with the TOML schema is intentional"
)]
pub struct ExtensionOptions {
    pub gfm: GfmOptions,
    pub definition_lists: bool,
    pub abbreviation_lists: bool,
    pub heading_attribute_lists: bool,
    pub block_attribute_lists: bool,
    pub myst: MystOptions,
    pub pandoc: PandocOptions,
}

impl Default for ExtensionOptions {
    fn default() -> Self {
        Self {
            gfm: GfmOptions::default(),
            definition_lists: true,
            abbreviation_lists: true,
            heading_attribute_lists: true,
            block_attribute_lists: true,
            myst: MystOptions::default(),
            pandoc: PandocOptions::default(),
        }
    }
}

/// Recognition toggles for GitHub Flavored Markdown extensions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct GfmOptions {
    pub autolinks: GfmAutolinkPolicy,
    pub tagfilter: bool,
}

impl Default for GfmOptions {
    fn default() -> Self {
        Self {
            autolinks: GfmAutolinkPolicy::UrlsAndEmails,
            tagfilter: true,
        }
    }
}

/// GFM extended-autolink recognition policy.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GfmAutolinkPolicy {
    Disabled,
    Urls,
    UrlsAndEmails,
}

/// Recognition toggles for `MyST`-flavoured extensions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one toggle per MyST construct; recognition gates are independent"
)]
pub struct MystOptions {
    pub directive_containers: bool,
    pub inline_roles: bool,
    pub substitution_references: bool,
    pub comments: bool,
}

impl Default for MystOptions {
    fn default() -> Self {
        Self {
            directive_containers: true,
            inline_roles: true,
            substitution_references: true,
            comments: true,
        }
    }
}

/// Recognition toggles for `Pandoc`-flavoured extensions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one toggle per Pandoc construct; recognition gates are independent"
)]
pub struct PandocOptions {
    pub fenced_divs: bool,
    pub short_form_divs: bool,
    pub inline_attribute_spans: bool,
}

impl Default for PandocOptions {
    fn default() -> Self {
        Self {
            fenced_divs: true,
            short_form_divs: true,
            inline_attribute_spans: true,
        }
    }
}

/// Input-boundary predicate: returns `true` when `s` carries a C0
/// control byte that mdwright treats as evidence the input is not
/// well-formed Markdown.
///
/// Allowed bytes inside `0x00..=0x1f`: TAB (`0x09`), LF (`0x0a`),
/// FF (`0x0c`), CR (`0x0d`). Everything else in C0 is rejected. DEL
/// (`0x7f`) is not rejected; `CommonMark` accepts it verbatim and real
/// documents occasionally carry it.
#[must_use]
pub fn contains_rejected_control_chars(s: &str) -> bool {
    s.bytes().any(|b| matches!(b, 0x00..=0x08 | 0x0B | 0x0E..=0x1F))
}

#[cfg(test)]
mod tests {
    use super::{Document, MathDelimiterSet, MathParseOptions, ParseOptions, contains_rejected_control_chars};

    #[test]
    fn control_char_predicate_accepts_clean_text() {
        assert!(!contains_rejected_control_chars(""));
        assert!(!contains_rejected_control_chars("# hello\n\nworld\n"));
        assert!(!contains_rejected_control_chars("tab\there\tand\nlf\n"));
        assert!(!contains_rejected_control_chars("ff:\x0c, cr:\r\n"));
        assert!(!contains_rejected_control_chars("café — 한글 — 𝓜"));
        assert!(!contains_rejected_control_chars("del:\x7f"));
    }

    #[test]
    fn control_char_predicate_rejects_c0_controls() {
        assert!(contains_rejected_control_chars("nul:\0"));
        assert!(contains_rejected_control_chars("bell:\x07"));
        assert!(contains_rejected_control_chars("unit-sep:\x1f"));
    }

    #[test]
    fn default_parse_options_do_not_recognize_dollar_math() -> Result<(), Box<dyn std::error::Error>> {
        let doc = Document::parse("x is $a + b$\n")?;
        assert!(doc.math_regions().is_empty());
        Ok(())
    }

    #[test]
    fn github_math_delimiters_recognize_dollar_math() -> Result<(), Box<dyn std::error::Error>> {
        let opts = ParseOptions::default().with_math(MathParseOptions {
            delimiters: MathDelimiterSet::Github,
        });
        let doc = Document::parse_with_options("x is $a + b$ and $$c + d$$\n", opts)?;
        assert_eq!(doc.math_regions().len(), 2);
        Ok(())
    }
}
