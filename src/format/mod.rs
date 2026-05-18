//! Markdown formatter.
//!
//! [`doc`] is the generic Wadler/Lindig `Doc` combinator (layout
//! IR + renderer). [`block`] turns a [`crate::tree::Tree`] into a
//! `Doc` for every block kind; [`inline`] does the same for inline
//! content (stubbed in this session — see its module docs).
//!
//! See Wadler, "A Prettier Printer" (1998); Lindig, "Strictly
//! Pretty" (2000); and the `prettyplease` crate for prior art.

pub(crate) mod block;
pub(crate) mod canonicalise;
pub(crate) mod doc;
pub(crate) mod document;
pub(crate) mod inline;
pub(crate) mod pretty;
pub(crate) mod semantic;
pub(crate) mod verbatim;
pub(crate) mod wrap;

use crate::config::{EndOfLine, TrailingNewline};

/// Apply the trailing-newline policy at the document boundary.
///
/// `Preserve` (the default) shapes the output to match the source's
/// trailing-newline run: one terminating `\n` if the source had any,
/// none otherwise. This is the only policy that survives
/// `Document::format_validated` on inputs ending in an indented or
/// fenced code block, where any LF the post-pass introduces lands
/// inside the code body on re-parse and changes the rendered HTML.
/// See `docs/architecture/pulldown-model.md` §2 for the trailing-blank-
/// line rule this post-pass exists to defend against.
///
/// The "did the source end with `\n`?" probe ignores trailing
/// horizontal whitespace (`' '` / `'\t'`). Pulldown treats a final
/// line of only spaces/tabs as a stripped trailing blank line: the
/// effective document ends one `\n` earlier than the byte count
/// suggests. Without the trim, source `\t|\n\t` (indented code,
/// content `|\n`, trailing tab-only blank line) reads as
/// "no trailing `\n`", so the boundary strips the code block's
/// content `\n` and the re-parse sees content `|` instead of `|\n`
/// (`fuzz_indented_code_trailing_ws_drop.in`).
///
/// `Strip` drops every trailing `\n`. `Ensure` forces exactly one
/// trailing `\n` — the pre-Preserve behaviour, now opt-in.
pub(crate) fn normalize_trailing_newline(out: &mut String, policy: TrailingNewline, source: &str) {
    while out.ends_with('\n') {
        let _ = out.pop();
    }
    let want_trailing = match policy {
        TrailingNewline::Preserve => source_has_effective_trailing_newline(source),
        TrailingNewline::Strip => false,
        TrailingNewline::Ensure => true,
    };
    if want_trailing {
        out.push('\n');
    }
}

/// True when the source's effective content ends with `\n`, ignoring
/// any final run of horizontal whitespace. See
/// [`normalize_trailing_newline`] for the rationale.
fn source_has_effective_trailing_newline(source: &str) -> bool {
    source.trim_end_matches([' ', '\t']).ends_with('\n')
}

/// Normalise every `\r\n` and lone `\r` in `out` to `\n`.
///
/// **Defensive safety net.** The load-bearing invariant lives on
/// [`crate::format::doc::text`]: every `Doc::Text` constructed from
/// source bytes canonicalises CR at construction, so the rendered
/// string already contains only `\n` terminators in practice. This
/// pass remains as cheap belt-and-braces (`.contains('\r')` early-out;
/// zero allocation when clean) in case a future emit site bypasses
/// the `text()` helper.
pub(crate) fn normalize_line_endings_lf(out: &mut String) {
    if !out.contains('\r') {
        return;
    }
    let normalized = out.replace("\r\n", "\n").replace('\r', "\n");
    *out = normalized;
}

/// Apply the end-of-line policy to a freshly-rendered `String`.
/// Caller invariant: `out` contains only `\n` line terminators
/// (enforced by [`normalize_line_endings_lf`] inside
/// `format_document`). Converting to CRLF is then a straightforward
/// replace; `Keep` adopts the source's first newline style.
pub(crate) fn apply_end_of_line(out: &mut String, policy: EndOfLine, source: &str) {
    let target = match policy {
        EndOfLine::Lf => "\n",
        EndOfLine::Crlf => "\r\n",
        EndOfLine::Keep => {
            if source.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
        }
    };
    if target == "\n" {
        return;
    }
    *out = out.replace('\n', target);
}

pub(crate) use document::format_document;
