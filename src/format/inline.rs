//! Inline-content dispatcher: walks a parent's inline children and
//! defers to each typed value's `pretty()` method.
//!
//! Emphasis delimiter resolution is contextual (it depends on the
//! immediately-preceding sibling and the first child), so this module
//! threads the resolution state through the walk before handing each
//! [`EmphasisRun`](crate::cm::inline::emphasis::EmphasisRun) /
//! [`StrongRun`](crate::cm::inline::emphasis::StrongRun) its chosen
//! delimiter.

use crate::cm::inline::emphasis::{EmphasisDelim, EmphasisRun, ResolveCtx, StrongRun};
use crate::cm::inline::link::flatten_body_doc;
use crate::cm::inline::strikethrough::Strikethrough;
use crate::format::doc::{Doc, concat, text};
use crate::format::pretty::PrettyCtx;
use crate::tree::{NodeId, NodeKind};

/// Render every inline child of `parent`.
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) fn pretty_inline_children<'a>(ctx: &PrettyCtx<'a>, parent: NodeId) -> Doc<'a> {
    let ids: Vec<NodeId> = ctx.tree.children(parent).collect();
    pretty_inline_children_for_ids(ctx, &ids)
}

/// Render an arbitrary slice of sibling inline nodes. Used by the
/// list-item renderer where a virtual paragraph's children must be
/// emitted without a real `Paragraph` parent in the tree.
pub(crate) fn pretty_inline_children_for_ids<'a>(
    ctx: &PrettyCtx<'a>,
    ids: &[NodeId],
) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::with_capacity(ids.len());
    let mut left_emphasis_delim: Option<EmphasisDelim> = None;
    for &cid in ids {
        let Some(node) = ctx.tree.node(cid) else {
            continue;
        };
        match &node.kind {
            NodeKind::Run(run) => parts.push(run.pretty()),
            NodeKind::CodeRun(code) => parts.push(code.pretty()),
            NodeKind::HtmlSpan(span) => parts.push(span.pretty()),
            NodeKind::Emphasis(run) => {
                let delim = run.resolve(ResolveCtx {
                    style: ctx.opts.italic(),
                    left_sibling_delim: left_emphasis_delim,
                    first_child_delim: first_child_strong_delim(ctx, cid),
                });
                let body = pretty_inline_children(ctx, cid);
                parts.push(EmphasisRun::pretty(body, delim));
                let _ = run;
                left_emphasis_delim = Some(delim);
                continue;
            }
            NodeKind::Strong(run) => {
                let delim = run.resolve(ResolveCtx {
                    style: ctx.opts.italic(),
                    left_sibling_delim: None,
                    first_child_delim: first_child_emphasis_delim(ctx, cid),
                });
                let body = pretty_inline_children(ctx, cid);
                parts.push(StrongRun::pretty(body, delim));
            }
            NodeKind::Strikethrough => {
                let body = pretty_inline_children(ctx, cid);
                parts.push(Strikethrough::pretty(body));
            }
            NodeKind::Link(run) => {
                let body = pretty_inline_children(ctx, cid);
                parts.push(run.pretty(body, ctx));
            }
            NodeKind::Image(run) => {
                let body = pretty_inline_children(ctx, cid);
                parts.push(run.pretty(body, ctx));
            }
            NodeKind::Autolink(run) => parts.push(run.pretty()),
            NodeKind::FootnoteReference(r) => parts.push(r.pretty()),
            NodeKind::TaskListMarker(_) => {
                // The list-item renderer prepends `[x] ` / `[ ] `; skip
                // the leaf so we don't emit it twice.
            }
            NodeKind::Document
            | NodeKind::Paragraph
            | NodeKind::Heading { .. }
            | NodeKind::BlockQuote
            | NodeKind::List { .. }
            | NodeKind::Item { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::HtmlBlock { .. }
            | NodeKind::ThematicBreak
            | NodeKind::Table { .. }
            | NodeKind::TableHead
            | NodeKind::TableRow
            | NodeKind::TableCell
            | NodeKind::FootnoteDefinition { .. }
            | NodeKind::Unknown { .. } => {
                debug_assert!(
                    matches!(&node.kind, NodeKind::Unknown { .. }),
                    "non-inline NodeKind reached pretty_inline_children: {:?}",
                    &node.kind
                );
                parts.push(text(ctx.tree.raw_text(cid)));
            }
        }
        left_emphasis_delim = None;
    }
    // `flatten_body_doc` is re-exported through `cm::inline::link`; the
    // unused import below keeps clippy from removing the visibility
    // contract this module relies on.
    let _ = flatten_body_doc;
    concat(parts)
}

/// `Some(d)` if the first child of `id` is a Strong run that will
/// resolve to delimiter `d`. Used to flip the outer Emphasis delimiter
/// so nested `*` / `**` do not fuse into `***`.
fn first_child_strong_delim(ctx: &PrettyCtx<'_>, id: NodeId) -> Option<EmphasisDelim> {
    let first = ctx.tree.children(id).next()?;
    let node = ctx.tree.node(first)?;
    let NodeKind::Strong(run) = &node.kind else {
        return None;
    };
    Some(run.resolve(ResolveCtx {
        style: ctx.opts.italic(),
        left_sibling_delim: None,
        first_child_delim: None,
    }))
}

/// Symmetric peer of [`first_child_strong_delim`] for the Strong
/// renderer: flips `**` to `__` when the first child is an Emphasis
/// that resolves to the same byte family.
fn first_child_emphasis_delim(ctx: &PrettyCtx<'_>, id: NodeId) -> Option<EmphasisDelim> {
    let first = ctx.tree.children(id).next()?;
    let node = ctx.tree.node(first)?;
    let NodeKind::Emphasis(run) = &node.kind else {
        return None;
    };
    Some(run.resolve(ResolveCtx {
        style: ctx.opts.italic(),
        left_sibling_delim: None,
        first_child_delim: None,
    }))
}
