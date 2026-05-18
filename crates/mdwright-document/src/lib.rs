#![forbid(unsafe_code)]

mod document;
mod heading;
mod ir;
mod line_index;
#[doc(hidden)]
pub mod parse;
mod refs;
mod source;
mod tree;
mod util;

pub use document::{Document, render_html};
pub use heading::{HeadingAttrs, find_attr_trailer_range};
pub use ir::{
    AllowScope, CodeBlock, Frontmatter, FrontmatterDelimiter, Heading, HtmlBlock, InlineCode, InlineHtml, LinkDef,
    ListGroup, ListItem, Suppression, SuppressionKind, TextSlice,
};
pub use line_index::{LineIndex, LineIndexError};
pub use mdwright_math::{MathError, MathRegion, MathSpan};
pub use refs::NormalisedLabel;
pub use source::{ByteSpan, CanonicalSource, OffsetMap, OriginalSpan, Source};
pub use tree::{Node, NodeId, NodeKind, TableAlign, Tree};

/// Markdown recognition policy.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct ParseOptions {
    extensions: ExtensionOptions,
}

impl ParseOptions {
    /// Extension-recognition toggles.
    #[must_use]
    pub fn extensions(&self) -> ExtensionOptions {
        self.extensions
    }

    /// Override extension-recognition toggles.
    #[must_use]
    pub fn with_extensions(mut self, extensions: ExtensionOptions) -> Self {
        self.extensions = extensions;
        self
    }
}

/// Per-extension recognition toggles.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one toggle per mdformat-mkdocs extension; the parallel naming with the TOML schema is intentional"
)]
pub struct ExtensionOptions {
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
            definition_lists: true,
            abbreviation_lists: true,
            heading_attribute_lists: true,
            block_attribute_lists: true,
            myst: MystOptions::default(),
            pandoc: PandocOptions::default(),
        }
    }
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
    use super::contains_rejected_control_chars;

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
}
