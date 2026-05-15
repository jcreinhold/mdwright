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

use pulldown_cmark::{Options, Parser, html};

use crate::config::FmtOptions;
use crate::diagnostic::Diagnostic;
use crate::format;
use crate::ir::{
    CodeBlock, Frontmatter, Heading, HtmlBlock, InlineCode, InlineHtml, Ir, LinkDef, ListGroup,
    Suppression, TextSlice,
};
use crate::line_index::LineIndex;
use crate::rule_set::RuleSet;
use crate::stdlib;
use crate::suppression::SuppressionMap;
use crate::tree::Tree;

/// Errors returned by [`Document::format_validated`].
#[derive(Debug, Clone)]
pub enum FormatError {
    /// Source and formatted output rendered to different HTML — the
    /// formatter changed the document's meaning. Carries both renders
    /// and the formatted text so callers can diff and decide how to
    /// surface the discrepancy.
    HtmlDivergence {
        source_html: String,
        formatted_html: String,
        formatted: String,
    },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HtmlDivergence { .. } => {
                write!(f, "formatter changed the document's HTML rendering")
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Render Markdown to HTML using the same parser options the IR uses.
/// Shared by the runtime gate and the GFM spec runner.
#[must_use]
pub fn render_html(source: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(source, opts);
    let mut out = String::with_capacity(source.len());
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
#[derive(Debug)]
pub struct Document<'a> {
    ir: Ir<'a>,
}

impl<'a> Document<'a> {
    /// Parse `source` into the IR. Infallible — pulldown-cmark
    /// recognises every byte sequence as Markdown.
    #[must_use]
    pub fn parse(source: &'a str) -> Self {
        Self {
            ir: Ir::parse(source),
        }
    }

    /// The full source string the document was parsed from.
    #[must_use]
    pub fn source(&self) -> &'a str {
        self.ir.source
    }

    /// Byte-offset → (line, column) translator. Use to construct
    /// diagnostics at arbitrary positions; [`Diagnostic::at`] is the
    /// usual sugar.
    ///
    /// [`Diagnostic::at`]: crate::Diagnostic::at
    #[must_use]
    pub fn line_index(&self) -> &LineIndex<'a> {
        self.ir.line_index()
    }

    /// Contiguous runs of prose text, with backslash escapes
    /// preserved. Each chunk is bounded by inline code, inline HTML,
    /// or a soft/hard line break — never crosses a code span.
    #[must_use]
    pub fn prose_chunks(&self) -> &[TextSlice<'a>] {
        &self.ir.prose_chunks
    }

    /// Inline code spans in source order. `text` excludes the
    /// surrounding backticks; `raw_range` covers them.
    #[must_use]
    pub fn inline_codes(&self) -> &[InlineCode<'a>] {
        &self.ir.inline_codes
    }

    /// TeX-style math regions detected in source (`\[ … \]`,
    /// `\( … \)`, optionally `$$ … $$` / `$ … $`). Lint rules that
    /// operate on prose (e.g., `latex-command`) consult this slice
    /// to skip diagnostics that fire inside math content — `\alpha`
    /// is intentional inside `\[ … \]` and a bug outside it.
    #[must_use]
    pub fn math_regions(&self) -> &[crate::format::math::MathRegion] {
        &self.ir.math_regions
    }

    /// Math open delimiters with no matching close. Surfaced by the
    /// `unbalanced-math-delim` lint rule.
    #[must_use]
    pub fn unclosed_math(&self) -> &[crate::format::math::UnclosedOpen] {
        &self.ir.unclosed_math
    }

    /// Fenced and indented code blocks in source order.
    #[must_use]
    pub fn code_blocks(&self) -> &[CodeBlock<'a>] {
        &self.ir.code_blocks
    }

    /// HTML blocks (`CommonMark` §4.6).
    #[must_use]
    pub fn html_blocks(&self) -> &[HtmlBlock<'a>] {
        &self.ir.html_blocks
    }

    /// Inline HTML tags (open, close, self-closing, comment).
    #[must_use]
    pub fn inline_html(&self) -> &[InlineHtml<'a>] {
        &self.ir.inline_html
    }

    /// ATX and setext headings with trimmed text and level.
    #[must_use]
    pub fn headings(&self) -> &[Heading<'a>] {
        &self.ir.headings
    }

    /// Lists in source order. Nested lists are separate entries.
    #[must_use]
    pub fn list_groups(&self) -> &[ListGroup] {
        &self.ir.list_groups
    }

    /// Link reference definitions discovered by post-parse scan.
    #[must_use]
    pub fn link_defs(&self) -> &[LinkDef<'a>] {
        &self.ir.link_defs
    }

    /// Frontmatter at the document head, if present. Carries both the
    /// raw slice and a tag for which delimiter (YAML `---` or TOML
    /// `+++`) the source used.
    #[must_use]
    pub fn frontmatter(&self) -> Option<&Frontmatter<'a>> {
        self.ir.frontmatter.as_ref()
    }

    /// The tree IR. Drives the formatter (sessions 06+); the linter
    /// keeps using the flat accessors above. Both IRs are built in a
    /// single pulldown-cmark event walk inside [`Document::parse`].
    #[must_use]
    pub fn tree(&self) -> &Tree<'a> {
        &self.ir.tree
    }

    /// Inline suppression directives parsed from `<!-- mdwright: … -->`
    /// HTML comments. Returned in source order. Consumed internally by
    /// [`Document::lint_with`]; exposed publicly so tooling can show
    /// users where their suppressions take effect.
    #[must_use]
    pub fn suppressions(&self) -> &[Suppression<'a>] {
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
            let (map, unknown) = SuppressionMap::build(&self.ir, &known);
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
    /// block-level serializer (inline content uses a source-verbatim
    /// stub in this session). Output trailing newline and line-ending
    /// style are taken from `opts`.
    #[must_use]
    pub fn format(&self, opts: &FmtOptions) -> String {
        format::format_document(
            self.source(),
            opts,
            self.tree(),
            self.ir.frontmatter.as_ref(),
            &self.ir.admonitions,
            &self.ir.math_regions,
        )
    }

    /// Reformat the document and verify the result renders to the same
    /// HTML as the source. The runtime gate catches accidental semantic
    /// drift (raw HTML insertion, dropped emphasis, malformed tables)
    /// that the cheap [`Document::format`] path cannot.
    ///
    /// Returns [`FormatError::HtmlDivergence`] when the formatted output
    /// renders to different HTML than the source. The caller should
    /// surface the error and skip writing the file.
    ///
    /// # Errors
    ///
    /// Returns an error if rendering source and formatted output to
    /// HTML produces different strings.
    pub fn format_validated(&self, opts: &FmtOptions) -> Result<String, FormatError> {
        let formatted = self.format(opts);
        let source_html = render_html(self.source());
        let formatted_html = render_html(&formatted);
        if source_html == formatted_html {
            Ok(formatted)
        } else {
            Err(FormatError::HtmlDivergence {
                source_html,
                formatted_html,
                formatted,
            })
        }
    }

    /// Apply every safe fix from `diags` to `source`, returning the
    /// repaired text and the count of edits applied. Overlapping
    /// safe fixes resolve right-to-left; the later edit wins.
    /// A free helper rather than a method because it doesn't need
    /// parser state.
    #[must_use]
    pub fn apply_safe_fixes(source: &str, diags: &[Diagnostic]) -> (String, usize) {
        let mut edits: Vec<(Range<usize>, &str)> = diags
            .iter()
            .filter_map(|d| {
                d.fix
                    .as_ref()
                    .filter(|f| f.safe)
                    .map(|f| (d.span.clone(), f.replacement.as_str()))
            })
            .collect();
        edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
        let mut out = source.to_owned();
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
