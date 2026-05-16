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
pub(crate) mod math;
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

/// Apply the end-of-line policy to a freshly-rendered `String`.
/// The renderer always emits `\n`, so converting to CRLF is a
/// straightforward replace; `Keep` adopts the source's first
/// newline style.
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
