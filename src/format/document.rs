//! Top-level document printer.
//!
//! Structural emit is pure preservation: every `.pretty()` method
//! reads source bytes, so a single render is a fixed point by
//! construction. There is no convergence loop and no per-site flank
//! lookup — the safety ladder and the two-pass machinery were
//! deleted once `FmtOptions` style consultation moved out of the
//! `.pretty()` layer (prompt 51). Style canonicalisation (when any
//! `FmtOptions` knob is set to a non-`Preserve` value) is a future
//! separate pass that operates on the structural output.
//!
//! Trailing-newline and end-of-line policies run as post-passes on
//! the rendered bytes — they are structural concerns, not style
//! rewrites.

use crate::cm::refs::ReferenceTable;
use crate::config::{FmtOptions, FormatMode};
use crate::format::block;
use crate::format::canonicalise;
use crate::format::doc::{self, RenderOptions};
use crate::format::pretty::PrettyCtx;
use crate::format::wrap::wrap_doc;
use crate::format::{apply_end_of_line, normalize_line_endings_lf, normalize_trailing_newline};
use crate::ir::{AdmonitionRegion, Frontmatter};
use crate::tree::Tree;

/// Front-end used by `Document::format`. Renders the tree IR into a
/// Markdown string in a single pass.
pub(crate) fn format_document<'a>(
    source: &'a str,
    opts: &'a FmtOptions,
    tree: &'a Tree,
    frontmatter: Option<&'a Frontmatter>,
    admonitions: &'a [AdmonitionRegion],
    refs: &'a ReferenceTable,
) -> String {
    let ctx = PrettyCtx {
        source,
        opts,
        tree,
        frontmatter,
        admonitions,
        refs,
    };
    let doc = if opts.mode() == FormatMode::Verbatim {
        doc::unbreakable(doc::text(source))
    } else {
        block::pretty_block_sequence(&ctx, tree.root())
    };
    let wrapped = wrap_doc(doc, opts.wrap());
    let mut out = doc::render(&wrapped, &RenderOptions);
    normalize_line_endings_lf(&mut out);
    if opts.has_any_canonicalisation() {
        canonicalise::canonicalise(&mut out, opts);
    }
    normalize_trailing_newline(&mut out, opts.trailing_newline(), source);
    apply_end_of_line(&mut out, opts.end_of_line(), source);
    out
}
