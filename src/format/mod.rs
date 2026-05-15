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
pub(crate) mod ctx;
pub(crate) mod doc;
pub(crate) mod escape;
pub(crate) mod inline;
pub(crate) mod math;
pub(crate) mod wrap;

use crate::config::{EndOfLine, FmtOptions};

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

/// Front-end used by `Document::format`. Renders the tree IR rooted
/// at `root` into a Markdown string.
pub(crate) fn format_document<'a>(
    source: &'a str,
    opts: &'a FmtOptions,
    tree: &'a crate::tree::Tree<'a>,
    frontmatter: Option<&'a crate::ir::Frontmatter<'a>>,
    admonitions: &'a [crate::ir::AdmonitionRegion<'a>],
    math_regions: &'a [crate::format::math::MathRegion],
) -> String {
    let ctx = ctx::Ctx {
        source,
        opts,
        tree,
        frontmatter,
        admonitions,
        math_regions,
    };
    let doc = block::render_block_sequence(&ctx, tree.root());
    let wrapped = wrap::wrap_doc(doc, opts.wrap());
    let mut out = doc::render(&wrapped, &doc::RenderOptions);
    normalize_trailing_newline(&mut out, opts.trailing_newline());
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    out
}
