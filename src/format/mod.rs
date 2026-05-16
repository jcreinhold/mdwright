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
pub(crate) mod doc;
pub(crate) mod document;
pub(crate) mod inline;
pub(crate) mod pretty;
pub(crate) mod verbatim;
pub(crate) mod wrap;

use crate::config::EndOfLine;

/// Ensure the rendered output ends in exactly one `\n` when
/// `trailing_newline` is true, or no trailing newline when it is
/// false.
pub(crate) fn normalize_trailing_newline(out: &mut String, trailing: bool) {
    while out.ends_with('\n') {
        let _ = out.pop();
    }
    if trailing {
        out.push('\n');
    }
}

/// Normalise every `\r\n` and lone `\r` in `out` to `\n`. After
/// this runs the string contains only LF line terminators, so the
/// end-of-line policy step can transform line endings uniformly
/// without worrying about CR bytes that leaked in from
/// source-verbatim emitters (e.g. `format/block.rs::verbatim_lines`,
/// admonition raw passthrough). Zero-cost when `out` has no `\r`.
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
