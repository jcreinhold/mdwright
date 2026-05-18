//! Top-level document formatter.
//!
//! Structural emit is the identity function: the canonicalised source bytes
//! are the round-trip-safe baseline by construction. The formatter exists to
//! apply opt-in transformations on top of that baseline — style
//! canonicalisation, line wrap, end-of-line conversion, trailing-newline
//! policy — each of which lives in the canonicalise pass (see
//! [`crate::format::canonicalise`]) or in a post-pass on the rendered
//! bytes.

use crate::FmtOptions;
use crate::format::rewrite;
use crate::format::{apply_end_of_line, normalize_line_endings_lf, normalize_trailing_newline};
use mdwright_document::ParseOptions;

/// Format `source` per `opts`. Returns the resulting string.
///
/// Default-options callers (every style knob `Preserve`, wrap `Keep`)
/// hit the identity early-out: the output is the canonicalised source,
/// modulo line-ending and trailing-newline policies. Opt-in
/// transformations route through the canonicalise pass; each rewrite
/// verifies before commit so a failed rewrite silently skips and the
/// source bytes survive.
pub(crate) fn format_document(source: &str, opts: &FmtOptions, parse_options: ParseOptions) -> String {
    let mut out = source.to_string();
    let has_canonicalisation = opts.has_any_canonicalisation();
    let has_wrap = !matches!(opts.wrap(), crate::Wrap::Keep);

    if has_canonicalisation || has_wrap {
        out = rewrite::apply_rewrites(&out, opts, parse_options);
    }
    // Defensive: `Source::canonical()` already normalises CR/CRLF to LF
    // before parse, so `source` here is LF-only in practice. The pass is a
    // cheap belt-and-braces (`.contains('\r')` early-out) in case a future
    // caller bypasses the canonicalisation.
    normalize_line_endings_lf(&mut out);
    normalize_trailing_newline(&mut out, opts.trailing_newline(), source);
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    out
}

#[cfg(test)]
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
        let once = crate::format_document(&mdwright_document::Document::parse(src), &opts);
        let twice = crate::format_document(&mdwright_document::Document::parse(&once), &opts);
        assert_eq!(once, twice);
    }
}
