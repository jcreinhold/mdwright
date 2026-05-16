//! Block-level orchestration: walks the tree's block children, applies
//! source-verbatim overlays (frontmatter, admonitions, math regions,
//! and root-level html / indented-code / verbatim-eligible paragraph),
//! and dispatches the remaining nodes through
//! [`TypedBlock::pretty`](crate::cm::block::TypedBlock::pretty).
//!
//! Per-construct serialisation logic lives next to each typed value
//! in `src/cm/block/*`. This module owns only the document-shape
//! decisions: how blocks are separated, when an overlay short-circuits
//! IR-driven emission, and the document-root tail passes for link
//! reference definitions and end-placed footnote definitions.

use std::ops::Range;

use crate::cm::block::paragraph::Paragraph;
use crate::config::{LinkDefStyle, Placement};
use crate::format::doc::{Doc, concat, hard_line, text, unbreakable};
use crate::format::pretty::PrettyCtx;
use crate::format::verbatim::emit_verbatim;
use crate::tree::{NodeId, NodeKind};

/// Render every direct block child of `parent` separated by a blank
/// line. Block helpers emit a trailing `HardLine`, so two consecutive
/// blocks produce one blank line between them; this routine inserts
/// the *second* hard line.
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) fn pretty_block_sequence<'a>(ctx: &PrettyCtx<'a>, parent: NodeId) -> Doc<'a> {
    let is_doc_root = parent == ctx.tree.root();
    let footnote_end = ctx.opts.footnote_placement() == Placement::End;
    let mut parts: Vec<Doc<'a>> = Vec::new();
    let mut emitted = 0usize;

    // Frontmatter: emit verbatim at the very top of the document.
    if is_doc_root
        && ctx.opts.preserve_frontmatter()
        && let Some(fm) = ctx.frontmatter
    {
        parts.push(unbreakable(verbatim_lines(fm.slice.text)));
        emitted = emitted.saturating_add(1);
    }

    let mut adm_idx = 0usize;
    let mut emitted_adm: Option<usize> = None;
    for child in ctx.tree.children(parent) {
        if is_doc_root
            && footnote_end
            && matches!(
                ctx.tree.node(child).map(|n| &n.kind),
                Some(NodeKind::FootnoteDefinition { .. })
            )
        {
            continue;
        }
        // Admonition overlay.
        if let Some(node) = ctx.tree.node(child) {
            let cr = node.raw_range.clone();
            while adm_idx < ctx.admonitions.len()
                && ctx.admonitions.get(adm_idx).map_or(0, |a| a.range.end) <= cr.start
            {
                adm_idx = adm_idx.saturating_add(1);
            }
            if let Some(region) = ctx.admonitions.get(adm_idx)
                && cr.start >= region.range.start
                && cr.start < region.range.end
            {
                if emitted_adm != Some(adm_idx) {
                    if emitted > 0 {
                        parts.push(hard_line());
                    }
                    parts.push(unbreakable(verbatim_lines(region.text)));
                    emitted = emitted.saturating_add(1);
                    emitted_adm = Some(adm_idx);
                }
                continue;
            }
        }
        // Math overlay: any block whose source range overlaps a math
        // region is emitted byte-verbatim.
        if let Some(node) = ctx.tree.node(child)
            && block_overlaps_math(ctx, &node.raw_range)
        {
            if emitted > 0 {
                parts.push(hard_line());
            }
            let raw = ctx.source.get(node.raw_range.clone()).unwrap_or("");
            parts.push(unbreakable(verbatim_lines(raw)));
            emitted = emitted.saturating_add(1);
            continue;
        }
        if emitted > 0 {
            parts.push(hard_line());
        }
        parts.push(pretty_block(ctx, child));
        emitted = emitted.saturating_add(1);
    }
    if is_doc_root {
        append_link_def_tail(ctx, &mut parts);
    }
    if is_doc_root && footnote_end {
        append_footnote_def_tail(ctx, &mut parts);
    }
    concat(parts)
}

/// Dispatch one block node through its typed value's `pretty()`. For
/// nodes without a typed payload (Document, Item, table sub-parts,
/// stray inline kinds at block position, Unknown) we fall back to
/// verbatim source emission.
///
/// At the document root, route block kinds whose only divergence from
/// the source is pulldown's re-tokenisation through `emit_verbatim`.
/// This is restricted to direct children of the root: nested-container
/// blocks have continuation prefixes embedded inside their `raw_range`,
/// which would double-emit under the surrounding blockquote/list
/// serializer.
pub(crate) fn pretty_block<'a>(ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
    let Some(node) = ctx.tree.node(id) else {
        return concat([]);
    };
    if ctx.tree.parent(id) == Some(ctx.tree.root()) {
        #[allow(clippy::wildcard_enum_match_arm)]
        match &node.kind {
            NodeKind::HtmlBlock { .. } => return emit_verbatim(ctx.tree, id),
            NodeKind::CodeBlock { fenced: false, .. } => return emit_verbatim(ctx.tree, id),
            NodeKind::Paragraph if Paragraph::is_verbatim_eligible(ctx, id) => {
                return emit_verbatim(ctx.tree, id);
            }
            _ => {}
        }
    }
    match &node.typed {
        Some(typed) => typed.pretty(ctx, id),
        None => concat([text(ctx.tree.raw_text(id)), hard_line()]),
    }
}

fn block_overlaps_math(ctx: &PrettyCtx<'_>, block: &Range<usize>) -> bool {
    ctx.math_regions
        .iter()
        .any(|r| r.range.start < block.end && block.start < r.range.end)
}

/// Build a `Doc` for `raw` that emits the input byte-verbatim with a
/// terminating newline. Single `Doc::Text` (`Cow::Borrowed`) plus a
/// terminating `HardLine`; the caller wraps in `unbreakable` so the
/// embedded newlines never enter a wrap run.
fn verbatim_lines(raw: &str) -> Doc<'_> {
    let trimmed = raw.trim_end_matches('\n');
    if trimmed.is_empty() {
        return hard_line();
    }
    concat([text(trimmed), hard_line()])
}

/// At the document root under [`Placement::End`], emit a tail block
/// containing every footnote definition collected in the tree, in
/// source order. Pulldown's HTML renderer emits these in source order
/// with `id` attributes derived from the label; sorting alphabetically
/// would change the HTML byte stream even when the rendered text is
/// identical, which fails `format_validated`.
fn append_footnote_def_tail<'a>(ctx: &PrettyCtx<'a>, parts: &mut Vec<Doc<'a>>) {
    let mut defs: Vec<(NodeId, usize)> = Vec::new();
    for child in ctx.tree.children(ctx.tree.root()) {
        let Some(node) = ctx.tree.node(child) else {
            continue;
        };
        if matches!(node.kind, NodeKind::FootnoteDefinition { .. }) {
            defs.push((child, node.raw_range.start));
        }
    }
    if defs.is_empty() {
        return;
    }
    defs.sort_by_key(|d| d.1);
    if !parts.is_empty() {
        parts.push(hard_line());
    }
    for (i, (child, _)) in defs.iter().enumerate() {
        if i > 0 {
            parts.push(hard_line());
        }
        parts.push(pretty_block(ctx, *child));
    }
}

/// At the document root, append every resolved link reference
/// definition in stable alphabetical order. [`ReferenceTable::insert`]
/// already enforces CM §4.7's "first definition wins" rule, so no
/// de-dup is needed here.
fn append_link_def_tail<'a>(ctx: &PrettyCtx<'a>, parts: &mut Vec<Doc<'a>>) {
    if ctx.refs.is_empty() {
        return;
    }
    if !parts.is_empty() {
        parts.push(hard_line());
    }
    let style = ctx.opts.link_def_style();
    let mut targets: Vec<_> = ctx.refs.iter().collect();
    targets.sort_by_key(|t| t.label_raw().to_ascii_lowercase());
    for target in targets {
        parts.push(render_link_ref_def(
            target.label_raw(),
            target.dest(),
            target.title(),
            style,
        ));
    }
}

fn render_link_ref_def<'a>(
    label: &str,
    dest: &str,
    title: Option<&str>,
    style: LinkDefStyle,
) -> Doc<'a> {
    let dest_rendered = crate::cm::inline::link::render_url_destination_owned(dest, style);
    let line = match title {
        Some(t) => format!("[{label}]: {dest_rendered} \"{t}\""),
        None => format!("[{label}]: {dest_rendered}"),
    };
    concat([text(line), hard_line()])
}
