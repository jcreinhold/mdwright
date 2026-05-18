//! Public parsed-document handle.
//!
//! `Document` is the deep façade over `crate::ir::Ir`. Rule authors
//! only see `Document`'s accessors; the IR's representation is free
//! to change without breaking the rule API. The data types returned
//! by accessors are defined once in `crate::ir` and re-exported from
//! `crate::lib` so users importing them directly get a stable path.

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

use pulldown_cmark::html;

use crate::cm::block::TypedBlock;
use crate::cm::block::list::ListBlock;
use crate::config::FmtOptions;
use crate::diagnostic::Diagnostic;
use crate::format;
use crate::ir::{
    CodeBlock, Frontmatter, Heading, HtmlBlock, InlineCode, InlineHtml, Ir, LinkDef, ListGroup, Suppression, TextSlice,
};
use crate::line_index::LineIndex;
use crate::parse;
use crate::rule_set::RuleSet;
use crate::source::{CanonicalSource, Source};
use crate::stdlib;
use crate::suppression::SuppressionMap;
use crate::tree::Tree;

/// Errors returned by [`Document::format_validated`].
#[derive(Debug, Clone)]
pub enum FormatError {
    /// The formatter changed the document's meaning — the formatted
    /// output's canonical pulldown-cmark event stream differs from the
    /// source's. Carries the formatted text and a one-line description
    /// of the first divergent event pair so callers can surface a
    /// useful diagnostic without re-running the comparison.
    SemanticDivergence {
        source: String,
        formatted: String,
        diff_summary: String,
    },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemanticDivergence { diff_summary, .. } => {
                write!(f, "formatter changed the document's meaning: {diff_summary}")
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Render Markdown to HTML using the same parser options the IR uses.
///
/// Kept as a public utility for callers (e.g. the GFM-spec test
/// harness needs the spec's reference HTML for non-formatter
/// purposes). The runtime gate itself no longer renders HTML — see
/// [`crate::format::semantic::semantically_equivalent`].
///
/// Inputs are routed through [`Source`] canonicalisation before
/// pulldown sees them (CM §2.1 CR / CRLF → LF, CM §2.3 NUL → U+FFFD),
/// matching what [`Document::parse`] does. Callers that need to render
/// raw bytes verbatim should reach for `pulldown_cmark::html` directly.
#[must_use]
pub fn render_html(source: &str) -> String {
    let src = Source::new(source);
    let canonical = CanonicalSource::from_source(&src);
    let parser = parse::events(canonical, parse::FORMATTER_OPTIONS);
    let mut out = String::with_capacity(canonical.as_str().len());
    html::push_html(&mut out, parser);
    out
}

/// Knobs that change `Document::lint_with`'s behaviour. All defaults
/// reproduce `Document::lint`'s behaviour.
#[derive(Copy, Clone, Debug)]
pub struct LintOptions {
    /// When `true` (the default), `<!-- mdwright: allow ... -->`
    /// comments filter diagnostics. Set `false` to see every
    /// diagnostic — used by the CLI's `--no-suppress` flag and by
    /// authors auditing where their suppressions take effect.
    pub respect_suppressions: bool,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            respect_suppressions: true,
        }
    }
}

/// A parsed Markdown document. Construct with [`Document::parse`];
/// query with the accessors; lint with [`Document::lint`].
///
/// `Document` owns a [`Source`] that holds both the caller-supplied
/// original bytes and the canonical view pulldown parses against
/// (CM §2.1 line endings + CM §2.3 NUL → U+FFFD). The IR's byte
/// ranges, the formatter, and the runtime HTML gate all see the
/// canonical bytes; diagnostics and [`Document::apply_safe_fixes`]
/// see the original.
#[derive(Debug)]
pub struct Document {
    source: Source,
    ir: Ir,
}

impl Document {
    /// Parse `source` into the IR. Infallible — pulldown-cmark
    /// recognises every byte sequence as Markdown.
    ///
    /// The library imposes **no** size cap; callers feeding untrusted
    /// input are responsible for bounding `source.len()` themselves.
    /// The `mdwright` CLI does this via `--max-input-bytes` (default
    /// 10 MB).
    #[must_use]
    #[tracing::instrument(level = "info", name = "Document::parse", skip(source), fields(len = source.len()))]
    pub fn parse(source: &str) -> Self {
        let source = Source::new(source);
        let ir = Ir::parse(CanonicalSource::from_source(&source));
        Self { source, ir }
    }

    /// The canonical source string the IR was parsed against. Equal
    /// to the caller's input when no CM §2.1 / §2.3 canonicalisation
    /// was needed; otherwise CRLF → LF and NUL → U+FFFD substitutions
    /// were applied.
    #[must_use]
    pub fn source(&self) -> &str {
        self.source.canonical()
    }

    /// The [`Source`] handle. Exposes both the original and canonical
    /// buffers plus the offset map between them.
    #[must_use]
    pub fn source_handle(&self) -> &Source {
        &self.source
    }

    /// Byte-offset → (line, column) translator. Use to construct
    /// diagnostics at arbitrary positions; [`Diagnostic::at`] is the
    /// usual sugar.
    ///
    /// [`Diagnostic::at`]: crate::Diagnostic::at
    #[must_use]
    pub fn line_index(&self) -> &LineIndex {
        self.ir.line_index()
    }

    /// Contiguous runs of prose text, with backslash escapes
    /// preserved. Each chunk is bounded by inline code, inline HTML,
    /// or a soft/hard line break — never crosses a code span.
    #[must_use]
    pub fn prose_chunks(&self) -> &[TextSlice] {
        &self.ir.prose_chunks
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
    pub fn math_regions(&self) -> &[crate::cm::math::MathRegion] {
        &self.ir.math_regions
    }

    /// Recogniser errors (unmatched delimiter opens, unmatched
    /// environment `\begin`). Surfaced by the `math/unbalanced-delim`
    /// and `math/unbalanced-env` lint rules.
    #[must_use]
    pub fn math_errors(&self) -> &[crate::cm::math::span::MathError] {
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

    /// Each [`ListGroup`] paired with its typed [`ListBlock`] view,
    /// when one was constructed (degenerate lists carry no typed
    /// view). Pairing is by `raw_range.start`, which is unique across
    /// lists in source order.
    pub(crate) fn typed_list_blocks(&self) -> Vec<(&ListGroup, &ListBlock)> {
        let mut typed_by_start: std::collections::HashMap<usize, &ListBlock> = std::collections::HashMap::new();
        let tree = self.tree();
        for id in tree.descendants(tree.root()) {
            let Some(node) = tree.node(id) else { continue };
            if let Some(TypedBlock::ListBlock(lb)) = &node.typed {
                typed_by_start.insert(node.raw_range.start, lb);
            }
        }
        self.ir
            .list_groups
            .iter()
            .filter_map(|g| typed_by_start.get(&g.raw_range.start).map(|lb| (g, *lb)))
            .collect()
    }

    /// Link reference definitions. Materialised on demand from the
    /// document's [`ReferenceTable`](crate::cm::refs::ReferenceTable);
    /// callers that hit this in a hot loop should cache the result.
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

    /// Frontmatter at the document head, if present. Carries both the
    /// raw slice and a tag for which delimiter (YAML `---` or TOML
    /// `+++`) the source used.
    #[must_use]
    pub fn frontmatter(&self) -> Option<&Frontmatter> {
        self.ir.frontmatter.as_ref()
    }

    /// The tree IR. Drives the formatter (sessions 06+); the linter
    /// keeps using the flat accessors above. Both IRs are built in a
    /// single pulldown-cmark event walk inside [`Document::parse`].
    #[must_use]
    pub fn tree(&self) -> &Tree {
        &self.ir.tree
    }

    /// Inline suppression directives parsed from `<!-- mdwright: … -->`
    /// HTML comments. Returned in source order. Consumed internally by
    /// [`Document::lint_with`]; exposed publicly so tooling can show
    /// users where their suppressions take effect.
    #[must_use]
    pub fn suppressions(&self) -> &[Suppression] {
        &self.ir.suppressions
    }

    /// Run every rule in `rules` over the document, respecting any
    /// `<!-- mdwright: … -->` suppression comments. Diagnostics are
    /// sorted by (line, column, rule-name). Equivalent to
    /// `self.lint_with(rules, LintOptions::default())`.
    #[must_use]
    pub fn lint(&self, rules: &RuleSet) -> Vec<Diagnostic> {
        self.lint_with(rules, LintOptions::default())
    }

    /// Run every rule in `rules` over the document under `opts`.
    /// The dispatcher stamps each diagnostic's `rule` and `advisory`
    /// fields from the owning rule, so rule implementations don't
    /// repeat their identity on every emit.
    #[must_use]
    pub fn lint_with(&self, rules: &RuleSet, opts: LintOptions) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for rule in rules.iter() {
            let before = out.len();
            rule.check(self, &mut out);
            let name_owned = rule.name().to_owned();
            let advisory = rule.is_advisory();
            for d in out.get_mut(before..).into_iter().flatten() {
                d.rule = Cow::Owned(name_owned.clone());
                d.advisory = advisory;
            }
        }

        if opts.respect_suppressions {
            let user_names: Vec<String> = rules.iter().map(|r| r.name().to_owned()).collect();
            let mut known: Vec<&str> = stdlib::names().collect();
            for n in &user_names {
                let s: &str = n.as_str();
                if !known.contains(&s) {
                    known.push(s);
                }
            }
            let (map, unknown) = SuppressionMap::build(self.source.canonical(), &self.ir, &known);
            out.retain(|d| !map.suppresses(&d.rule, &d.span));
            out.extend(unknown);
        }

        out.sort_by(|a, b| {
            a.line
                .cmp(&b.line)
                .then(a.column.cmp(&b.column))
                .then_with(|| a.rule.cmp(&b.rule))
        });
        out
    }

    /// Reformat the document.
    ///
    /// Produces a Markdown string by walking the tree IR through the
    /// structural-preserve formatter: every `.pretty()` emits source
    /// bytes, so a single render is a fixed point by construction.
    /// Use [`Self::format_validated`] when the caller needs the
    /// runtime semantic-equivalence gate as well.
    #[must_use]
    #[tracing::instrument(level = "info", name = "Document::format", skip_all, fields(out_len = tracing::field::Empty))]
    pub fn format(&self, opts: &FmtOptions) -> String {
        let out = format::format_document(
            self.source.canonical(),
            opts,
            self.tree(),
            self.ir.frontmatter.as_ref(),
            &self.ir.admonitions,
            &self.ir.abbreviations,
            &self.ir.block_attrs,
            &self.ir.directives,
            &self.ir.comments,
            &self.ir.inline_overlays,
            &self.ir.refs,
        );
        tracing::Span::current().record("out_len", out.len());
        out
    }

    /// Reformat the document and verify the result is stable under a
    /// second pass with the same options ("idempotence-on-mode"). The
    /// runtime gate catches accidental semantic drift (raw HTML
    /// insertion, dropped emphasis, malformed tables) that the cheap
    /// [`Document::format`] path cannot.
    ///
    /// Equivalence is defined on canonicalised pulldown-cmark event
    /// streams (see [`crate::format::semantic`]) — soft-break
    /// positions and prose whitespace runs are normalised; verbatim
    /// regions (code blocks, inline code, raw HTML, math) compare
    /// byte-for-byte.
    ///
    /// # Why idempotence-on-mode, not source-vs-formatted
    ///
    /// Most options — wrap, italic style, list marker — round-trip the
    /// source's math regions byte-for-byte, so the formatted output's
    /// math events match the source's. But [`crate::MathRender::Dollar`]
    /// intentionally rewrites `\[ … \]` to `$$ … $$`; under those
    /// options the source's math events and the formatted output's
    /// math events differ by construction. A source-vs-formatted gate
    /// would reject every dollar-rendered document, so the gate
    /// asserts the weaker but still-strict property: formatting the
    /// output a second time with the same options must produce the
    /// same canonical event stream. Round-1 → round-2 divergence is
    /// still a hard failure.
    ///
    /// Returns [`FormatError::SemanticDivergence`] with a short
    /// summary of the first differing event when the gate fails. The
    /// caller should surface the error and skip writing the file.
    /// `source` on the error carries the *formatted* bytes (the input
    /// to round 2), since that is the side of the comparison the
    /// caller can inspect.
    ///
    /// # Errors
    ///
    /// Returns an error if formatting the output a second time
    /// produces a different canonical event stream.
    pub fn format_validated(&self, opts: &FmtOptions) -> Result<String, FormatError> {
        let formatted = self.format(opts);
        let twice = Self::parse(&formatted).format(opts);
        match crate::format::semantic::first_divergence(&formatted, &twice) {
            None => Ok(formatted),
            Some(diff_summary) => Err(FormatError::SemanticDivergence {
                source: formatted.clone(),
                formatted: twice,
                diff_summary,
            }),
        }
    }

    /// Apply every safe fix from `diags` to this document's
    /// **original** source, returning the repaired text and the count
    /// of edits applied. Diagnostic `span` fields carry canonical-byte
    /// offsets (the bytes the IR was built from); this method
    /// translates each span back to its original-byte range so the
    /// repaired text preserves CRLF endings and original NUL bytes
    /// outside the spans the fix touches.
    ///
    /// Overlapping safe fixes resolve right-to-left; the later edit
    /// wins.
    #[must_use]
    pub fn apply_safe_fixes(&self, diags: &[Diagnostic]) -> (String, usize) {
        use crate::source::ByteSpan;
        let mut edits: Vec<(Range<usize>, &str)> = diags
            .iter()
            .filter_map(|d| {
                let fix = d.fix.as_ref().filter(|f| f.safe)?;
                // Translate the canonical span the diagnostic carries
                // to an original span so the edit lands at the bytes
                // the caller's file actually has.
                let canon = ByteSpan::new(u32::try_from(d.span.start).ok()?, u32::try_from(d.span.end).ok()?);
                let orig = self.source.to_original(canon);
                Some((orig.range(), fix.replacement.as_str()))
            })
            .collect();
        edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        let mut out = self.source.original().to_owned();
        let mut applied = 0usize;
        let mut last_start = usize::MAX;
        for (range, replacement) in edits {
            if range.end > last_start {
                continue;
            }
            out.replace_range(range.clone(), replacement);
            last_start = range.start;
            applied = applied.saturating_add(1);
        }
        (out, applied)
    }
}
