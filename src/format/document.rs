//! Top-level document formatter.
//!
//! Structural emit is the identity function: the canonicalised source bytes
//! are the round-trip-safe baseline by construction. The formatter exists to
//! apply opt-in transformations on top of that baseline — style
//! canonicalisation, line wrap, end-of-line conversion, trailing-newline
//! policy — each of which lives in the canonicalise pass (see
//! [`crate::format::canonicalise`]) or in a post-pass on the rendered
//! bytes.

use crate::config::FmtOptions;
use crate::format::canonicalise;
use crate::format::wrap_pass;
use crate::format::{apply_end_of_line, normalize_line_endings_lf, normalize_trailing_newline};

/// Format `source` per `opts`. Returns the resulting string.
///
/// Default-options callers (every style knob `Preserve`, wrap `Keep`)
/// hit the identity early-out: the output is the canonicalised source,
/// modulo line-ending and trailing-newline policies. Opt-in
/// transformations route through the canonicalise pass; each rewrite
/// verifies via per-paragraph reparse so a failed rewrite silently
/// skips and the source bytes survive.
pub(crate) fn format_document(source: &str, opts: &FmtOptions) -> String {
    let mut out = source.to_string();
    if opts.has_any_canonicalisation() {
        canonicalise::canonicalise(&mut out, opts);
    }
    wrap_pass::wrap_paragraphs(&mut out, opts.wrap());
    // Defensive: `Source::canonical()` already normalises CR/CRLF to LF
    // before parse, so `source` here is LF-only in practice. The pass is a
    // cheap belt-and-braces (`.contains('\r')` early-out) in case a future
    // caller bypasses the canonicalisation.
    normalize_line_endings_lf(&mut out);
    normalize_trailing_newline(&mut out, opts.trailing_newline(), source);
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    out
}
