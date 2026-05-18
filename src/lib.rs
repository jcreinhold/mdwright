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
mod incremental;
mod ir;
mod line_index;
pub mod lsp;
mod parse;
mod rule;
mod rule_set;
mod source;
pub mod stdlib;
mod suppression;
mod tree;
mod util;

pub use config::{
    Config, ConfigError, EndOfLine, FmtOptions, FormatMode, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions,
    OrderedListStyle, Placement, StrongStyle, ThematicStyle, TrailingNewline, Wrap,
};
pub use diagnostic::{DOCS_URL_DEFAULT, Diagnostic, Fix, Severity, Snippet, docs_url, rule_doc_url};
pub use discover::discover_markdown;
pub use document::{Document, FormatError, LintOptions, render_html};
pub use format::semantic::semantically_equivalent;
pub use incremental::CheckpointTable;
pub use ir::{
    AllowScope, CodeBlock, Frontmatter, FrontmatterDelimiter, Heading, HtmlBlock, InlineCode, InlineHtml, LinkDef,
    ListGroup, ListItem, Suppression, SuppressionKind, TextSlice,
};
pub use line_index::LineIndex;
pub use rule::LintRule;
pub use rule_set::{DuplicateRuleName, RuleSet};

/// Format the smallest set of whole top-level blocks that covers
/// `range` in `source`.
///
/// Range formatting exists to make editor latency proportional to the
/// edit, not the document. The headline consumer is the LSP server
/// (`textDocument/rangeFormatting`, `textDocument/onTypeFormatting`):
/// given the byte range the user is editing, mdwright re-emits the
/// covering blocks and returns just those bytes.
///
/// `range` is snapped outward to whole-block boundaries — empty,
/// out-of-bounds, partial-block, and frontmatter-only ranges all
/// resolve to a well-defined slice (empty when the request is wholly
/// past the source end). Errors are defined out of existence.
///
/// **Substring contract.** For every well-formed `source` that does
/// not contain document-scope reorderable constructs (link definitions,
/// footnote definitions), and every range `r`:
///
/// ```text
/// format(source, opts).contains(&format_range(source, opts, r))
/// ```
///
/// The proptest at `tests/properties.rs::range_format_is_substring_of_whole`
/// fences this contract.
///
/// **Caveat — link / footnote definitions.** Link defs (`[label]: dest`)
/// and footnote defs (`[^label]: …`) are document-scope: the
/// formatter may move them to a canonical location per [`LinkDefStyle`]
/// or [`Placement`]. A slice containing both a reference and its def
/// keeps them adjacent in the range output; the whole-document output
/// may insert other blocks between them. Range output is still a valid
/// formatting of the covered blocks — just not necessarily a verbatim
/// substring. The LSP server's expected workflow (per-keystroke range
/// format, periodic whole-doc save) absorbs this without user-visible
/// drift in practice.
///
/// For callers that range-format the same source many times (the LSP
/// case), build a [`CheckpointTable`] once and call
/// [`format_range_with_checkpoints`] instead — that skips the per-call
/// boundary scan. For one-shot CLI use this function is the right
/// entry point.
///
/// # Example
///
/// ```
/// use mdwright::{format_range, FmtOptions};
///
/// let src = "first\n\nsecond\n\nthird\n";
/// let opts = FmtOptions::default();
/// // The "second" paragraph starts at byte 7.
/// let out = format_range(src, &opts, 8..10);
/// assert!(out.contains("second"));
/// assert!(!out.contains("first"));
/// assert!(!out.contains("third"));
/// ```
#[must_use]
pub fn format_range(source: &str, opts: &FmtOptions, range: std::ops::Range<usize>) -> String {
    let table = CheckpointTable::build(source);
    format_range_with_checkpoints(source, opts, &table, range)
}

/// Range-format using a pre-built [`CheckpointTable`].
///
/// Identical to [`format_range`] but skips rebuilding the boundary
/// table. The LSP server holds one `CheckpointTable` per open buffer,
/// rebuilt on each `didChange` notification (a single event walk, no
/// IR construction).
///
/// `table` must have been built from the same `source` bytes the
/// caller is passing here. Passing a stale table produces output that
/// may not satisfy the substring contract — the LSP must rebuild the
/// table on every edit.
#[must_use]
pub fn format_range_with_checkpoints(
    source: &str,
    opts: &FmtOptions,
    table: &CheckpointTable,
    range: std::ops::Range<usize>,
) -> String {
    let req_lo = u32::try_from(range.start).unwrap_or(0);
    let req_hi = u32::try_from(range.end).unwrap_or(u32::MAX);
    let snapped = table.snap_to_block_boundaries(req_lo..req_hi);
    let lo = snapped.start as usize;
    let hi = snapped.end as usize;
    let slice = source.get(lo..hi).unwrap_or("");
    Document::parse(slice).format(opts)
}

/// Input-boundary predicate: returns `true` when `s` carries a C0
/// control byte that mdwright treats as evidence the input is not
/// well-formed Markdown.
///
/// Allowed bytes inside `0x00..=0x1f`: TAB (`0x09`), LF (`0x0a`),
/// FF (`0x0c`), CR (`0x0d`). Everything else in C0 is rejected. DEL
/// (`0x7f`) is *not* rejected; `CommonMark` accepts it verbatim and
/// real documents occasionally carry it.
///
/// This is an *opt-in* policy. The library never refuses such input
/// on its own — `Document::parse` follows `CommonMark` §2.3 and
/// substitutes NUL with U+FFFD. The CLI's `--reject-control-chars`
/// flag and the coverage-guided fuzz harness call this predicate to
/// decline inputs the operator considers suspect (and that pulldown
/// would silently rewrite, breaking the idempotence oracle).
pub fn contains_rejected_control_chars(s: &str) -> bool {
    s.bytes().any(|b| matches!(b, 0x00..=0x08 | 0x0B | 0x0E..=0x1F))
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Document, LintRule, RuleSet, contains_rejected_control_chars};

    #[test]
    fn control_char_predicate_accepts_clean_text() {
        assert!(!contains_rejected_control_chars(""));
        assert!(!contains_rejected_control_chars("# hello\n\nworld\n"));
        assert!(!contains_rejected_control_chars("tab\there\tand\nlf\n"));
        // FF and CR are spec-legal in Markdown source.
        assert!(!contains_rejected_control_chars("ff:\x0c, cr:\r\n"));
        // High Unicode is fine.
        assert!(!contains_rejected_control_chars("café — 한글 — 𝓜"));
        // DEL is not in the reject set.
        assert!(!contains_rejected_control_chars("del:\x7f"));
    }

    #[test]
    fn control_char_predicate_rejects_c0_controls() {
        assert!(contains_rejected_control_chars("nul:\0"));
        assert!(contains_rejected_control_chars("eot:\x04"));
        // Vertical tab (0x0B) is rejected.
        assert!(contains_rejected_control_chars("vt:\x0b"));
        // SO/SI through US (0x0e..0x1f).
        assert!(contains_rejected_control_chars("so:\x0e"));
        assert!(contains_rejected_control_chars("us:\x1f"));
    }

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
    fn format_range_returns_substring_of_whole() {
        let src = "first\n\nsecond\n\nthird\n";
        let opts = super::FmtOptions::default();
        let whole = Document::parse(src).format(&opts);
        let mid = super::format_range(src, &opts, 8..10);
        assert!(whole.contains(&mid), "substring contract: whole={whole:?} mid={mid:?}");
        assert!(mid.contains("second"), "mid={mid:?}");
        assert!(!mid.contains("first"), "mid={mid:?}");
        assert!(!mid.contains("third"), "mid={mid:?}");
    }

    #[test]
    fn format_range_empty_at_eof_is_empty() {
        let src = "a\n";
        let opts = super::FmtOptions::default();
        let past = super::format_range(src, &opts, 99..100);
        assert_eq!(past, "");
    }

    #[test]
    fn format_range_with_cached_table_matches_whole() {
        let src = "alpha\n\nbeta\n\ngamma\n";
        let opts = super::FmtOptions::default();
        let table = super::CheckpointTable::build(src);
        let whole = Document::parse(src).format(&opts);
        let beta_at = src.find("beta").unwrap_or(0);
        let part = super::format_range_with_checkpoints(src, &opts, &table, beta_at..beta_at + 1);
        assert!(whole.contains(&part), "whole={whole:?} part={part:?}");
        assert!(part.contains("beta"));
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
