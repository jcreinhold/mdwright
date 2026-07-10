#![forbid(unsafe_code)]

mod format;
mod incremental;
mod options;

use std::fmt;
use std::ops::Range;

pub use format::semantic::{first_divergence, semantically_equivalent};
pub use incremental::CheckpointTable;
pub use options::{
    BlankLine, EndOfLine, FmtOptions, HeadingAttrsStyle, ItalicStyle, LinkDefStyle, ListContinuationIndent,
    ListMarkerStyle, MathOptions, MathRender, OrderedListStyle, Placement, StrongStyle, TableStyle, ThematicStyle,
    TrailingNewline, Wrap, WrapStrategy,
};

use mdwright_document::{Document, ParseError};

/// Errors returned by [`format_validated`].
#[derive(Debug, Clone)]
pub enum FormatError {
    /// Source or formatted output could not be parsed safely.
    Parse(ParseError),
    /// The formatter changed the document's meaning.
    SemanticDivergence {
        source: String,
        formatted: String,
        diff_summary: String,
    },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "{err}"),
            Self::SemanticDivergence { diff_summary, .. } => {
                write!(f, "formatter changed the document's meaning: {diff_summary}")
            }
        }
    }
}

impl std::error::Error for FormatError {}

impl From<ParseError> for FormatError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

/// Aggregate formatter rewrite counts.
///
/// The report is intentionally coarse: it exposes whether rewrite
/// transactions are being attempted, committed, or rejected without
/// exposing candidates, owners, snapshots, or parser internals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatReport {
    pub rewrite_candidates: usize,
    pub rewrite_committed: usize,
    pub rewrite_committed_wrap: usize,
    pub rewrite_committed_style: usize,
    pub rewrite_rejected_overlap: usize,
    pub rewrite_rejected_verification: usize,
    pub rewrite_rejected_convergence: usize,
    pub rewrite_skipped_wrap: usize,
}

/// Format a parsed document.
#[must_use]
#[tracing::instrument(level = "info", name = "format_document", skip_all, fields(out_len = tracing::field::Empty))]
pub fn format_document(doc: &Document, opts: &FmtOptions) -> String {
    let out = format::document::format_document(doc, opts);
    tracing::Span::current().record("out_len", out.len());
    out
}

/// Format a parsed document and return aggregate rewrite metrics.
#[must_use]
pub fn format_document_with_report(doc: &Document, opts: &FmtOptions) -> (String, FormatReport) {
    format::document::format_document_with_report(doc, opts)
}

/// Parse and format Markdown source with default parse options.
///
/// # Errors
///
/// Returns [`FormatError::Parse`] if source parsing fails.
pub fn format_source(source: &str, opts: &FmtOptions) -> Result<String, FormatError> {
    Ok(format_document(&Document::parse(source)?, opts))
}

/// Format and verify that a second pass is semantically stable.
///
/// # Errors
///
/// Returns an error if formatting the output a second time produces a
/// different canonical event stream.
pub fn format_validated(doc: &Document, opts: &FmtOptions) -> Result<String, FormatError> {
    format_validated_with_report(doc, opts).map(|(formatted, _report)| formatted)
}

/// Format, return aggregate metrics, and verify that a second pass is
/// semantically stable.
///
/// # Errors
///
/// Returns an error if formatting the output a second time produces a
/// different canonical event stream.
pub fn format_validated_with_report(doc: &Document, opts: &FmtOptions) -> Result<(String, FormatReport), FormatError> {
    let (formatted, report) = format_document_with_report(doc, opts);
    let formatted_doc = Document::parse_with_options(&formatted, doc.parse_options())?;
    let twice = format_document(&formatted_doc, opts);
    match format::semantic::first_divergence_with_options(&formatted, &twice, doc.parse_options())? {
        None => Ok((formatted, report)),
        Some(diff_summary) => Err(FormatError::SemanticDivergence {
            source: formatted.clone(),
            formatted: twice,
            diff_summary,
        }),
    }
}

/// Format the smallest set of whole top-level blocks that covers
/// `range` in `source`.
#[must_use]
pub fn format_range(doc: &Document, opts: &FmtOptions, range: Range<usize>) -> String {
    let table = CheckpointTable::from_document(doc);
    format_range_with_checkpoints(doc, opts, &table, range)
}

/// Range-format using a pre-built [`CheckpointTable`].
#[must_use]
pub fn format_range_with_checkpoints(
    doc: &Document,
    opts: &FmtOptions,
    table: &CheckpointTable,
    range: Range<usize>,
) -> String {
    let req_lo = u32::try_from(range.start).unwrap_or(0);
    let req_hi = u32::try_from(range.end).unwrap_or(u32::MAX);
    let snapped = table.snap_to_block_boundaries(req_lo..req_hi);
    let lo = snapped.start as usize;
    let hi = snapped.end as usize;
    let source = doc.source();
    let slice = source.get(lo..hi).unwrap_or("");
    match Document::parse_with_options(slice, doc.parse_options()) {
        Ok(slice_doc) => format::document::format_document(&slice_doc, opts),
        Err(err) => {
            tracing::warn!(
                target: "mdwright::format",
                error = %err,
                "range-format slice parse failed; leaving slice bytes unchanged",
            );
            format::document::format_unparsed_source(slice, opts)
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use mdwright_document::{ExtensionOptions, ParseOptions};

    #[test]
    fn format_document_uses_document_parse_options() {
        let source = "# Heading {.class #id}\n";
        let opts = FmtOptions::default().with_heading_attrs(HeadingAttrsStyle::Canonicalise);

        let enabled = Document::parse(source).expect("fixture parses");
        assert_eq!(format_document(&enabled, &opts), "# Heading {#id .class}\n");

        let parse_options = ParseOptions::default().with_extensions(ExtensionOptions {
            heading_attribute_lists: false,
            ..ExtensionOptions::default()
        });
        let disabled = Document::parse_with_options(source, parse_options).expect("fixture parses");
        assert_eq!(format_document(&disabled, &opts), source);
    }

    #[test]
    fn mdformat_profile_reports_no_candidates_when_no_sites_match() {
        let doc = Document::parse("plain paragraph\n").expect("fixture parses");
        let (formatted, report) = format_document_with_report(&doc, &FmtOptions::mdformat());
        assert_eq!(formatted, doc.source());
        assert_eq!(report, FormatReport::default());
    }

    #[test]
    fn default_table_normal_form_keeps_table_free_fast_path() {
        let doc = Document::parse("plain paragraph\n").expect("fixture parses");
        let (formatted, report) = format_document_with_report(&doc, &FmtOptions::default());
        assert_eq!(formatted, doc.source());
        assert_eq!(report, FormatReport::default());
    }
}
