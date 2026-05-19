//! Top-level document formatter.
//!
//! Structural emit is the identity function: the canonicalised source bytes
//! are the round-trip-safe baseline by construction. The formatter exists to
//! apply opt-in transformations on top of that baseline — style
//! canonicalisation, line wrap, end-of-line conversion, trailing-newline
//! policy — each of which lives in the canonicalise pass (see
//! [`crate::format::canonicalise`]) or in a post-pass on the rendered
//! bytes.

use crate::format::rewrite;
use crate::format::{apply_end_of_line, normalize_line_endings_lf, normalize_trailing_newline};
use crate::{FmtOptions, FormatReport};
use mdwright_document::Document;

/// Format `source` per `opts`. Returns the resulting string.
///
/// Default-options callers (every style knob `Preserve`, wrap `Keep`)
/// hit the identity early-out: the output is the canonicalised source,
/// modulo line-ending and trailing-newline policies. Opt-in
/// transformations route through the canonicalise pass; each rewrite
/// verifies before commit so a failed rewrite silently skips and the
/// source bytes survive.
pub(crate) fn format_document(doc: &Document, opts: &FmtOptions) -> String {
    format_document_with_report(doc, opts).0
}

pub(crate) fn format_document_with_report(doc: &Document, opts: &FmtOptions) -> (String, FormatReport) {
    let source = doc.source();
    let mut out = source.to_string();
    let mut report = FormatReport::default();
    let has_canonicalisation = opts.has_any_canonicalisation();
    let has_wrap = !matches!(opts.wrap(), crate::Wrap::Keep);

    if has_canonicalisation || has_wrap {
        match rewrite::apply_rewrites(doc, opts) {
            Ok((rewritten, rewrite_report)) => {
                out = rewritten;
                report = rewrite_report;
            }
            Err(err) => {
                tracing::warn!(
                    target: "mdwright::rewrite",
                    error = %err,
                    "rewrite snapshot parse failed; leaving source bytes unchanged",
                );
            }
        }
    }
    // Defensive: `Source::canonical()` already normalises CR/CRLF to LF
    // before parse, so `source` here is LF-only in practice. The pass is a
    // cheap belt-and-braces (`.contains('\r')` early-out) in case a future
    // caller bypasses the canonicalisation.
    normalize_line_endings_lf(&mut out);
    normalize_trailing_newline(&mut out, opts.trailing_newline(), source);
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    (out, report)
}

pub(crate) fn format_unparsed_source(source: &str, opts: &FmtOptions) -> String {
    let mut out = source.to_string();
    normalize_line_endings_lf(&mut out);
    normalize_trailing_newline(&mut out, opts.trailing_newline(), source);
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::{
        FmtOptions, ItalicStyle, LinkDefStyle, ListMarkerStyle, MathOptions, OrderedListStyle, StrongStyle,
        ThematicStyle, Wrap,
    };

    fn all_underscore_and_dash_opts() -> FmtOptions {
        FmtOptions::default()
            .with_wrap(Wrap::At(120))
            .with_math(MathOptions {
                normalise: true,
                ..MathOptions::default()
            })
            .with_italic(ItalicStyle::Underscore)
            .with_strong(StrongStyle::Underscore)
            .with_list_marker(ListMarkerStyle::Dash)
            .with_thematic_break(ThematicStyle::Dash)
            .with_ordered_list(OrderedListStyle::Consistent)
            .with_link_def_style(LinkDefStyle::Angle)
    }

    #[test]
    fn canonicalise_and_wrap_converge_when_wrap_exposes_delimiters() {
        let src = "!*-\r__+*\r\\\n}";
        let opts = all_underscore_and_dash_opts();
        let once = crate::format_document(&mdwright_document::Document::parse(src).expect("source parses"), &opts);
        let twice = crate::format_document(
            &mdwright_document::Document::parse(&once).expect("output parses"),
            &opts,
        );
        assert_eq!(once, twice);
    }
}
