#![forbid(unsafe_code)]

mod format;
mod incremental;
mod options;

use std::fmt;
use std::ops::Range;

pub use format::semantic::{first_divergence, semantically_equivalent};
pub use incremental::CheckpointTable;
pub use options::{
    EndOfLine, FmtOptions, HeadingAttrsStyle, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, MathRender,
    OrderedListStyle, Placement, StrongStyle, ThematicStyle, TrailingNewline, Wrap,
};

use mdwright_document::Document;

/// Errors returned by [`format_validated`].
#[derive(Debug, Clone)]
pub enum FormatError {
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
            Self::SemanticDivergence { diff_summary, .. } => {
                write!(f, "formatter changed the document's meaning: {diff_summary}")
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Format a parsed document.
#[must_use]
#[tracing::instrument(level = "info", name = "format_document", skip_all, fields(out_len = tracing::field::Empty))]
pub fn format_document(doc: &Document, opts: &FmtOptions) -> String {
    let out = format::document::format_document(doc.source(), opts);
    tracing::Span::current().record("out_len", out.len());
    out
}

/// Parse and format Markdown source with default parse options.
#[must_use]
pub fn format_source(source: &str, opts: &FmtOptions) -> String {
    format_document(&Document::parse(source), opts)
}

/// Format and verify that a second pass is semantically stable.
///
/// # Errors
///
/// Returns an error if formatting the output a second time produces a
/// different canonical event stream.
pub fn format_validated(doc: &Document, opts: &FmtOptions) -> Result<String, FormatError> {
    let formatted = format_document(doc, opts);
    let twice = format_source(&formatted, opts);
    match first_divergence(&formatted, &twice) {
        None => Ok(formatted),
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
pub fn format_range(source: &str, opts: &FmtOptions, range: Range<usize>) -> String {
    let table = CheckpointTable::build(source);
    format_range_with_checkpoints(source, opts, &table, range)
}

/// Range-format using a pre-built [`CheckpointTable`].
#[must_use]
pub fn format_range_with_checkpoints(
    source: &str,
    opts: &FmtOptions,
    table: &CheckpointTable,
    range: Range<usize>,
) -> String {
    let req_lo = u32::try_from(range.start).unwrap_or(0);
    let req_hi = u32::try_from(range.end).unwrap_or(u32::MAX);
    let snapped = table.snap_to_block_boundaries(req_lo..req_hi);
    let lo = snapped.start as usize;
    let hi = snapped.end as usize;
    let slice = source.get(lo..hi).unwrap_or("");
    format_source(slice, opts)
}
