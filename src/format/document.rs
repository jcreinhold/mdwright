//! Top-level document printer.
//!
//! Builds a [`PrettyCtx`] from the formatter inputs, hands the
//! tree-root off to [`block::pretty_block_sequence`] (which dispatches
//! through each [`TypedBlock`](crate::cm::block::TypedBlock)'s
//! `pretty()` method), and post-processes the rendered string for the
//! configured trailing-newline + EOL policies.

use crate::cm::refs::ReferenceTable;
use crate::config::{FmtOptions, FormatMode};
use crate::format::block;
use crate::format::doc::{self, RenderOptions};
use crate::format::math::MathRegion;
use crate::format::pretty::PrettyCtx;
use crate::format::wrap::wrap_doc;
use crate::format::{apply_end_of_line, normalize_line_endings_lf, normalize_trailing_newline};
use crate::ir::{AdmonitionRegion, Frontmatter};
use crate::tree::Tree;

/// Front-end used by `Document::format`. Renders the tree IR rooted
/// at `root` into a Markdown string.
pub(crate) fn format_document<'a>(
    source: &'a str,
    opts: &'a FmtOptions,
    tree: &'a Tree<'a>,
    frontmatter: Option<&'a Frontmatter<'a>>,
    admonitions: &'a [AdmonitionRegion<'a>],
    math_regions: &'a [MathRegion],
    refs: &'a ReferenceTable,
) -> String {
    let ctx = PrettyCtx {
        source,
        opts,
        tree,
        frontmatter,
        admonitions,
        math_regions,
        refs,
    };
    let doc = if opts.mode() == FormatMode::Verbatim {
        // Emit the entire document source as one borrowed slice.
        // Trailing-newline and end-of-line policies still apply at
        // the document boundary; no block-level rewrite runs.
        doc::unbreakable(doc::text(source))
    } else {
        block::pretty_block_sequence(&ctx, tree.root())
    };
    let wrapped = wrap_doc(doc, opts.wrap());
    let mut out = doc::render(&wrapped, &RenderOptions);
    // Verbatim source-passthrough emitters can leak `\r` bytes from
    // CR-containing source into the rendered string. Pulldown's
    // block detection is line-ending-sensitive (a lone CR is not the
    // same as a CRLF or LF for fence-opener detection), so leaving
    // those bytes in would let mdwright's output reparse to a
    // different structure on the next format pass. Normalise here,
    // once, at the document chokepoint.
    normalize_line_endings_lf(&mut out);
    normalize_trailing_newline(&mut out, opts.trailing_newline());
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    out
}
