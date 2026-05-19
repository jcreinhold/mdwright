//! Public parsed-document handle.
//!
//! `Document` is the deep facade over `crate::ir::Ir`. Rule authors
//! only see `Document`'s accessors; the IR's representation is free
//! to change without breaking the rule API. The data types returned
//! by accessors are defined once in `crate::ir` and re-exported from
//! this crate so users importing them directly get a stable path.

use pulldown_cmark::html;
use std::ops::Range;

use crate::ParseError;
use crate::ParseOptions;
use crate::format_facts::FormatFacts;
use crate::gfm::apply_gfm_render_policy;
use crate::ir::{
    BlockCheckpointFact, CodeBlock, Frontmatter, Heading, HtmlBlock, InlineCode, InlineHtml, Ir, LinkDef, ListGroup,
    Suppression, TextSlice,
};
use crate::line_index::LineIndex;
use crate::parse;
use crate::render::{RenderOptions, RenderProfile, render_cmark_gfm_html};
use crate::source::ByteSpan;
use crate::source::{CanonicalSource, Source};
use mdwright_math::{MathError, MathRegion};

/// Render Markdown to HTML using the same parser options the IR uses.
///
/// Kept as a public utility for callers that need the same `CommonMark`
/// rendering policy as document recognition.
///
/// Inputs are routed through document-owned source canonicalisation before
/// pulldown sees them (CM §2.1 CR / CRLF → LF, CM §2.3 NUL → U+FFFD),
/// matching what [`Document::parse`] does. Callers that need to render
/// raw bytes verbatim should reach for `pulldown_cmark::html` directly.
///
/// # Errors
///
/// Returns [`ParseError`] if parser execution cannot safely recognise
/// the canonicalised source.
pub fn render_html(source: &str) -> Result<String, ParseError> {
    render_html_with_options(source, ParseOptions::default())
}

/// Render Markdown to HTML under explicit recognition options.
///
/// # Errors
///
/// Returns [`ParseError`] if parser execution cannot safely recognise
/// the canonicalised source.
pub fn render_html_with_options(source: &str, opts: ParseOptions) -> Result<String, ParseError> {
    render_html_with_render_options(source, opts, RenderOptions::default())
}

/// Render Markdown to HTML under explicit recognition and render options.
///
/// # Errors
///
/// Returns [`ParseError`] if parser execution cannot safely recognise
/// the canonicalised source.
pub fn render_html_with_render_options(
    source: &str,
    opts: ParseOptions,
    render: RenderOptions,
) -> Result<String, ParseError> {
    let src = Source::new(source);
    let canonical = CanonicalSource::from_source(&src);
    let events = parse::collect_events_with_offsets(canonical, parse::options(opts))?;
    let events = apply_gfm_render_policy(canonical.as_str(), events, opts.extensions().gfm);
    match render.profile() {
        RenderProfile::Pulldown => {
            let mut out = String::with_capacity(canonical.as_str().len());
            html::push_html(&mut out, events.into_iter());
            Ok(out)
        }
        RenderProfile::CmarkGfm => Ok(render_cmark_gfm_html(events)),
    }
}

/// A parsed Markdown document.
///
/// Construct with [`Document::parse`] or [`Document::parse_with_options`]
/// and query with the accessors. Linting and formatting are operations
/// owned by their respective crates.
///
/// `Document` owns both the caller-supplied original bytes and the
/// canonical view pulldown parses against
/// (CM §2.1 line endings + CM §2.3 NUL → U+FFFD). The IR's byte
/// ranges and semantic inventories see the canonical bytes; diagnostic
/// renderers and safe-fix application map those spans back to the
/// original.
#[derive(Debug)]
pub struct Document {
    source: Source,
    ir: Ir,
    parse_options: ParseOptions,
}

impl Document {
    /// Parse `source` into the IR.
    ///
    /// The library imposes **no** size cap; callers feeding untrusted
    /// input are responsible for bounding `source.len()` themselves.
    /// The `mdwright` CLI does this via `--max-input-bytes` (default
    /// 10 MB).
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if parser execution cannot safely
    /// recognise the canonicalised source.
    #[tracing::instrument(level = "info", name = "Document::parse", skip(source), fields(len = source.len()))]
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        Self::parse_with_options(source, ParseOptions::default())
    }

    /// Parse `source` under explicit recognition options.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if parser execution cannot safely
    /// recognise the canonicalised source under `opts`.
    pub fn parse_with_options(source: &str, opts: ParseOptions) -> Result<Self, ParseError> {
        let source = Source::new(source);
        let ir = Ir::parse(&source, opts)?;
        Ok(Self {
            source,
            ir,
            parse_options: opts,
        })
    }

    /// Recognition policy used to build this document.
    #[must_use]
    pub fn parse_options(&self) -> ParseOptions {
        self.parse_options
    }

    /// The canonical source string the IR was parsed against. Equal
    /// to the caller's input when no CM §2.1 / §2.3 canonicalisation
    /// was needed; otherwise CRLF → LF and NUL → U+FFFD substitutions
    /// were applied.
    #[must_use]
    pub fn source(&self) -> &str {
        self.source.canonical()
    }

    /// The caller's original source bytes, before `CommonMark`
    /// line-ending and NUL canonicalisation.
    #[must_use]
    pub fn original_source(&self) -> &str {
        self.source.original()
    }

    /// Translate a canonical byte range into the caller's original
    /// source coordinates.
    #[must_use]
    pub fn canonical_to_original_range(&self, range: Range<usize>) -> Range<usize> {
        let span = ByteSpan::from_range(range);
        self.source.to_original(span).range()
    }

    /// Byte-offset → (line, column) translator.
    #[must_use]
    pub fn line_index(&self) -> &LineIndex {
        self.ir.line_index()
    }

    pub(crate) fn format_facts(&self) -> &FormatFacts {
        &self.ir.format_facts
    }

    /// Contiguous runs of prose text, with backslash escapes
    /// preserved. Each chunk is bounded by inline code, inline HTML,
    /// or a soft/hard line break — never crosses a code span.
    #[must_use]
    pub fn prose_chunks(&self) -> &[TextSlice] {
        &self.ir.prose_chunks
    }

    /// `CommonMark` and GFM autolinks recognised in source order.
    #[must_use]
    pub fn autolinks(&self) -> &[crate::AutolinkFact] {
        &self.ir.autolinks
    }

    /// Inline code spans in source order. `text` excludes the
    /// surrounding backticks; `raw_range` covers them.
    #[must_use]
    pub fn inline_codes(&self) -> &[InlineCode] {
        &self.ir.inline_codes
    }

    /// TeX-style math regions detected in source (`\[ … \]`,
    /// `\( … \)`, `\begin{env} … \end{env}`, optionally
    /// `$$ … $$` / `$ … $`). Lint rules that operate on prose
    /// (e.g., `latex-command`) consult this slice to skip
    /// diagnostics that fire inside math content — `\alpha` is
    /// intentional inside `\[ … \]` and a bug outside it.
    #[must_use]
    pub fn math_regions(&self) -> &[MathRegion] {
        &self.ir.math_regions
    }

    /// Recogniser errors (unmatched delimiter opens, unmatched
    /// environment `\begin`). Surfaced by the `math/unbalanced-delim`
    /// and `math/unbalanced-env` lint rules.
    #[must_use]
    pub fn math_errors(&self) -> &[MathError] {
        &self.ir.math_errors
    }

    /// Fenced and indented code blocks in source order.
    #[must_use]
    pub fn code_blocks(&self) -> &[CodeBlock] {
        &self.ir.code_blocks
    }

    /// HTML blocks (`CommonMark` §4.6).
    #[must_use]
    pub fn html_blocks(&self) -> &[HtmlBlock] {
        &self.ir.html_blocks
    }

    /// Inline HTML tags (open, close, self-closing, comment).
    #[must_use]
    pub fn inline_html(&self) -> &[InlineHtml] {
        &self.ir.inline_html
    }

    /// ATX and setext headings with trimmed text and level.
    #[must_use]
    pub fn headings(&self) -> &[Heading] {
        &self.ir.headings
    }

    /// Lists in source order. Nested lists are separate entries.
    #[must_use]
    pub fn list_groups(&self) -> &[ListGroup] {
        &self.ir.list_groups
    }

    /// Each [`ListGroup`] paired with the tree-derived tightness for
    /// the matching structural list node. Pairing is by
    /// `raw_range.start`, which is unique across lists in source
    /// order.
    #[must_use]
    pub fn list_tightness_view(&self) -> Vec<(&ListGroup, bool)> {
        self.ir
            .list_groups
            .iter()
            .filter_map(|g| {
                self.ir
                    .list_tightness
                    .iter()
                    .find(|(start, _)| *start == g.raw_range.start)
                    .map(|(_, tight)| (g, *tight))
            })
            .collect()
    }

    /// Link reference definitions. Materialised on demand from the
    /// document's internal reference table; callers that hit this in a
    /// hot loop should cache the result.
    /// The returned slice borrows from `self` (not from source), so the
    /// `&str` fields have the document's borrow lifetime.
    #[must_use]
    pub fn link_defs(&self) -> Vec<LinkDef<'_>> {
        self.ir
            .refs
            .iter()
            .map(|t| LinkDef {
                label: t.label_raw.as_str(),
                dest: t.dest.as_str(),
                title: t.title.as_deref(),
                raw_range: t.raw_range.clone(),
            })
            .collect()
    }

    /// Top-level block checkpoints in canonical source coordinates.
    #[must_use]
    pub fn block_checkpoints(&self) -> &[BlockCheckpointFact] {
        &self.ir.block_checkpoints
    }

    /// Source ranges for links, images, and autolinks that should be
    /// treated as link-like regions by prose rules.
    #[must_use]
    pub fn link_like_ranges(&self) -> &[Range<usize>] {
        &self.ir.link_like_ranges
    }

    /// Frontmatter at the document head, if present. Carries both the
    /// raw slice and a tag for which delimiter (YAML `---` or TOML
    /// `+++`) the source used.
    #[must_use]
    pub fn frontmatter(&self) -> Option<&Frontmatter> {
        self.ir.frontmatter.as_ref()
    }

    /// Inline suppression directives parsed from `<!-- mdwright: … -->`
    /// HTML comments. Returned in source order so linting and tooling
    /// can show users where suppressions take effect.
    #[must_use]
    pub fn suppressions(&self) -> &[Suppression] {
        &self.ir.suppressions
    }
}
