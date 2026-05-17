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

use crate::cm::block::TypedBlock;
use crate::cm::block::paragraph::Paragraph;
use crate::cm::math::MathRegion;
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
        parts.push(unbreakable(verbatim_lines(&fm.slice.text)));
        emitted = emitted.saturating_add(1);
    }

    let mut adm_idx = 0usize;
    let mut emitted_adm: Option<usize> = None;
    // The bullet character of the most recently emitted adjacent
    // unordered list in this sequence, if any. Pulldown distinguishes
    // adjacent lists by their marker char (CM §5.2); per-list
    // bullet-style normalisation can otherwise merge what the source
    // emitted as two separate lists. The state resets to `None`
    // whenever any non-unordered-list block intervenes.
    let mut prev_unordered_bullet: Option<u8> = None;
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
                    parts.push(unbreakable(verbatim_lines(&region.text)));
                    emitted = emitted.saturating_add(1);
                    emitted_adm = Some(adm_idx);
                    prev_unordered_bullet = None;
                }
                continue;
            }
        }
        // Math overlay. Two cases:
        //   * The block is *entirely* math (display `\[…\]` / `$$…$$`
        //     or a `\begin{env}…\end{env}` standing on its own) — we
        //     render it through `MathSpan::pretty` for whitespace
        //     normalisation and ampersand alignment.
        //   * The block merely *contains* one or more math regions
        //     (typical: a paragraph with inline `\(x\)` fragments) —
        //     we emit the whole block verbatim. The recogniser cannot
        //     reliably distinguish prose backslash escapes like
        //     `\(\)` / `\[\\\]` (GFM spec ex. 308) from genuine
        //     inline math; touching only some of those bytes would
        //     drift the round-trip, so we keep the safe overlay.
        if let Some(node) = ctx.tree.node(child) {
            let hits = math_regions_in(ctx, &node.raw_range);
            if !hits.is_empty() {
                if emitted > 0 {
                    parts.push(hard_line());
                }
                let doc = if let Some(region) = whole_block_math(&hits, &node.raw_range, ctx.source)
                {
                    let mut pieces: Vec<Doc<'a>> = Vec::with_capacity(3);
                    pieces.push(region.span.pretty(ctx, &region.range));
                    pieces.push(hard_line());
                    unbreakable(concat(pieces))
                } else {
                    let raw = ctx.source.get(node.raw_range.clone()).unwrap_or("");
                    unbreakable(verbatim_lines(raw))
                };
                parts.push(doc);
                emitted = emitted.saturating_add(1);
                prev_unordered_bullet = None;
                continue;
            }
        }
        // Unordered-list adjacency: resolve the bullet against the
        // previous adjacent unordered list's emitted bullet, so the
        // formatter cannot merge two source-distinct lists into one
        // by normalising both to the same marker. CM §5.2 ends a list
        // when the marker character changes; bullet-style
        // normalisation that ignored adjacency was the fuzz-found
        // `+\n-` bug class.
        if let Some(node) = ctx.tree.node(child)
            && let Some(TypedBlock::ListBlock(list)) = &node.typed
            && list.is_unordered()
        {
            if emitted > 0 {
                parts.push(hard_line());
            }
            let bullet = list.resolve_unordered_bullet(ctx.opts, prev_unordered_bullet);
            parts.push(list.pretty_with_bullet(ctx, child, Some(bullet)));
            emitted = emitted.saturating_add(1);
            prev_unordered_bullet = Some(bullet);
            continue;
        }
        if emitted > 0 {
            parts.push(hard_line());
        }
        parts.push(pretty_block(ctx, child));
        emitted = emitted.saturating_add(1);
        prev_unordered_bullet = None;
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
    if ctx.tree.parent(id) == Some(ctx.tree.root()) && root_verbatim_safe(ctx, id) {
        #[allow(clippy::wildcard_enum_match_arm)]
        match &node.kind {
            NodeKind::HtmlBlock { .. } => return emit_verbatim(ctx.source, ctx.tree, id),
            NodeKind::CodeBlock { fenced: false, .. } => {
                return emit_verbatim(ctx.source, ctx.tree, id);
            }
            NodeKind::Paragraph if Paragraph::is_verbatim_eligible(ctx, id) => {
                return emit_verbatim(ctx.source, ctx.tree, id);
            }
            _ => {}
        }
    }
    match &node.typed {
        Some(typed) => typed.pretty(ctx, id),
        None => concat([
            text(ctx.tree.raw_text(ctx.source, id).to_owned()),
            hard_line(),
        ]),
    }
}

/// A root block is verbatim-safe iff its raw source contains no `\r`.
/// Pulldown's block-starter detection (fence opener, indented-code
/// blank-line rule, ATX heading, …) is line-ending-sensitive, so
/// CR-bearing source emitted verbatim and then LF-normalised at the
/// document chokepoint (`format::normalize_line_endings_lf`) could
/// reparse to a different shape than the input. IR-driven emission
/// (`typed.pretty()`) does not have this hazard because it
/// materialises the block as canonical LF Markdown.
fn root_verbatim_safe(ctx: &PrettyCtx<'_>, id: NodeId) -> bool {
    let Some(node) = ctx.tree.node(id) else {
        return false;
    };
    !ctx.source
        .get(node.raw_range.clone())
        .unwrap_or("")
        .contains('\r')
}

/// Math regions overlapping `block` in source order. The returned
/// references point into `ctx.math_regions`; their `range` fields are
/// source-absolute.
fn math_regions_in<'b>(ctx: &'b PrettyCtx<'_>, block: &Range<usize>) -> Vec<&'b MathRegion> {
    ctx.math_regions
        .iter()
        .filter(|r| r.range.start < block.end && block.start < r.range.end)
        .collect()
}

/// `Some(region)` iff exactly one math region covers all non-blank
/// bytes of `block`. Whole-block coverage is the safety condition
/// that lets us swap byte-verbatim emission for `MathSpan::pretty`:
/// the source bytes outside the math region are at most leading /
/// trailing whitespace, so a different (normalised) render of the
/// math cannot rearrange surrounding prose.
fn whole_block_math<'b>(
    hits: &[&'b MathRegion],
    block: &Range<usize>,
    source: &str,
) -> Option<&'b MathRegion> {
    if hits.len() != 1 {
        return None;
    }
    let region = hits.first().copied()?;
    let head = source.get(block.start..region.range.start)?;
    let tail = source.get(region.range.end..block.end)?;
    if head
        .bytes()
        .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        && tail
            .bytes()
            .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
    {
        Some(region)
    } else {
        None
    }
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
