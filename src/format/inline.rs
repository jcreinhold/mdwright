//! Inline-content dispatcher: walks a parent's inline children and
//! defers to each typed value's `pretty()` method.
//!
//! Emphasis delimiter resolution is contextual (it depends on the
//! immediately-preceding sibling and the first child), so this module
//! threads the resolution state through the walk before handing each
//! [`EmphasisRun`](crate::cm::inline::emphasis::EmphasisRun) /
//! [`StrongRun`](crate::cm::inline::emphasis::StrongRun) its chosen
//! delimiter.
//!
//! Paragraph-context emitters (`Paragraph::pretty`, list-item virtual
//! paragraphs) instead call [`pretty_paragraph_inline`], which walks
//! the same children but additionally threads paragraph-safety state
//! through the walk and applies `CommonMark` line-start escapes to
//! `RunPart::Text` payloads before any `Doc` construction. This keeps
//! escape decisions operating on full text payloads (so a `# foo`
//! continuation line still escapes correctly) while moving the
//! decision down a layer from the old Doc-tree walker.

use crate::cm::block::paragraph_safety::{
    LineContext, escape_for_block_start, escape_for_paragraph_interrupt, escape_setext_underline,
};
use crate::cm::inline::emphasis::{EmphasisDelim, ResolveCtx};
use crate::cm::inline::run::{InlineRun, RunPart};
use crate::cm::inline::strikethrough::Strikethrough;
use crate::format::doc::{Doc, concat, hard_line, line, prose, text};
use crate::format::emit_safety::{FlankCtx, RunKind, emit_emphasis_safely};
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
pub(crate) fn pretty_inline_children_for_ids<'a>(ctx: &PrettyCtx<'a>, ids: &[NodeId]) -> Doc<'a> {
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
                let source_slice = ctx.tree.raw_text(ctx.source, cid);
                let flank = flank_ctx_for(ctx, cid);
                parts.push(emit_emphasis_safely(
                    body,
                    delim,
                    RunKind::Emphasis,
                    source_slice,
                    flank,
                ));
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
                let source_slice = ctx.tree.raw_text(ctx.source, cid);
                let flank = flank_ctx_for(ctx, cid);
                parts.push(emit_emphasis_safely(body, delim, RunKind::Strong, source_slice, flank));
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
            NodeKind::Math(span) => parts.push(span.pretty(ctx, &node.raw_range)),
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
                parts.push(text(ctx.tree.raw_text(ctx.source, cid).to_owned()));
            }
        }
        left_emphasis_delim = None;
    }
    concat(parts)
}

// ============================================================
// Paragraph-safety variant
// ============================================================
//
// `pretty_paragraph_inline` is the safety-aware sibling of
// `pretty_inline_children`. Used by paragraph and list-item emitters.
// The walker threads `ParagraphSafetyState` so that every
// `RunPart::Text` payload encountered at a logical line-start gets
// the CommonMark escape pass applied to its full content before any
// `Doc` is built. State continues across emphasis / strong / link /
// image children, and clears across opaque inline nodes (code,
// autolink, raw HTML, math, footnote reference).

/// Render every inline child of `parent` as the body of a paragraph
/// or list-item virtual paragraph. Applies the line-start escape pass
/// at the run-stream layer.
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) fn pretty_paragraph_inline<'a>(ctx: &PrettyCtx<'a>, parent: NodeId) -> Doc<'a> {
    let ids: Vec<NodeId> = ctx.tree.children(parent).collect();
    pretty_paragraph_inline_for_ids(ctx, &ids)
}

/// Like [`pretty_paragraph_inline`] but takes an explicit slice of
/// inline-child IDs. Mirrors [`pretty_inline_children_for_ids`] for
/// the list-item virtual-paragraph case.
pub(crate) fn pretty_paragraph_inline_for_ids<'a>(ctx: &PrettyCtx<'a>, ids: &[NodeId]) -> Doc<'a> {
    let mut state = ParagraphSafetyState::initial();
    let mut parts: Vec<Doc<'a>> = Vec::with_capacity(ids.len());
    let mut left_emphasis_delim: Option<EmphasisDelim> = None;
    walk_paragraph_inline(ctx, ids, &mut parts, &mut state, &mut left_emphasis_delim);
    trim_edge_breaks(&mut parts);
    concat(parts)
}

/// Paragraph-safety state. Tracks where in the assembled body we are
/// so that each `RunPart::Text` can pick the right escape set
/// (block-start vs paragraph-interrupt vs setext-underline).
///
/// Four boolean flags rather than a single enum because the four
/// dimensions are independent: `at_line_start` distinguishes hard-vs-
/// soft break context, `after_break` widens the line-start window
/// after either kind of break, `prev_line_had_text` gates the §5
/// paragraph-interrupt rules, and `this_line_has_text` latches the
/// previous one. Packing these into an enum would either explode the
/// variant count or hide the orthogonality.
#[derive(Copy, Clone, Debug)]
#[allow(clippy::struct_excessive_bools, reason = "four independent line-context flags")]
struct ParagraphSafetyState {
    /// True at the start of the body or immediately after a
    /// `HardLine`. The next text fragment may begin a CM block.
    at_line_start: bool,
    /// True when the previous part was any kind of break (`Line` or
    /// `HardLine`). Differentiates "line-start after a soft break"
    /// (paragraph-interrupt rules) from "line-start after a hard
    /// break" (full block-start rules).
    after_break: bool,
    /// True when the line that ended at the most recent break carried
    /// at least one text fragment. The CM §5 paragraph-interrupt
    /// rules only fire when the previous line was non-empty.
    prev_line_had_text: bool,
    /// True once any text or opaque content has been emitted on the
    /// current line. Latched into `prev_line_had_text` when a break
    /// is observed.
    this_line_has_text: bool,
}

impl ParagraphSafetyState {
    const fn initial() -> Self {
        Self {
            at_line_start: true,
            after_break: true,
            prev_line_had_text: false,
            this_line_has_text: false,
        }
    }

    /// Update the state to reflect having emitted non-break content
    /// (a text fragment or an opaque inline like inline code).
    fn note_content(&mut self) {
        self.at_line_start = false;
        self.after_break = false;
        self.this_line_has_text = true;
    }

    /// Update the state after emitting a `Doc::HardLine`.
    fn note_hard_line(&mut self) {
        self.at_line_start = true;
        self.after_break = true;
        self.prev_line_had_text = self.this_line_has_text;
        self.this_line_has_text = false;
    }

    /// Update the state after emitting a `Doc::Line` (soft break).
    fn note_soft_break(&mut self) {
        self.at_line_start = false;
        self.after_break = true;
        self.prev_line_had_text = self.this_line_has_text;
        self.this_line_has_text = false;
    }
}

fn walk_paragraph_inline<'a>(
    ctx: &PrettyCtx<'a>,
    ids: &[NodeId],
    out: &mut Vec<Doc<'a>>,
    state: &mut ParagraphSafetyState,
    left_emphasis_delim: &mut Option<EmphasisDelim>,
) {
    let last_idx = ids.len().saturating_sub(1);
    for (i, &cid) in ids.iter().enumerate() {
        let Some(node) = ctx.tree.node(cid) else {
            continue;
        };
        let has_next_sibling = i < last_idx;
        match &node.kind {
            NodeKind::Run(run) => {
                emit_run_with_safety(run, has_next_sibling, out, state);
            }
            NodeKind::CodeRun(code) => {
                out.push(code.pretty());
                state.note_content();
            }
            NodeKind::HtmlSpan(span) => {
                out.push(span.pretty());
                state.note_content();
            }
            NodeKind::Emphasis(run) => {
                let delim = run.resolve(ResolveCtx {
                    style: ctx.opts.italic(),
                    left_sibling_delim: *left_emphasis_delim,
                    first_child_delim: first_child_strong_delim(ctx, cid),
                });
                let body = build_inline_body_with_safety(ctx, cid, state);
                let source_slice = ctx.tree.raw_text(ctx.source, cid);
                let flank = flank_ctx_for(ctx, cid);
                out.push(emit_emphasis_safely(
                    body,
                    delim,
                    RunKind::Emphasis,
                    source_slice,
                    flank,
                ));
                *left_emphasis_delim = Some(delim);
                continue;
            }
            NodeKind::Strong(run) => {
                let delim = run.resolve(ResolveCtx {
                    style: ctx.opts.italic(),
                    left_sibling_delim: None,
                    first_child_delim: first_child_emphasis_delim(ctx, cid),
                });
                let body = build_inline_body_with_safety(ctx, cid, state);
                let source_slice = ctx.tree.raw_text(ctx.source, cid);
                let flank = flank_ctx_for(ctx, cid);
                out.push(emit_emphasis_safely(body, delim, RunKind::Strong, source_slice, flank));
            }
            NodeKind::Strikethrough => {
                let body = build_inline_body_with_safety(ctx, cid, state);
                out.push(Strikethrough::pretty(body));
            }
            NodeKind::Link(run) => {
                let body = build_inline_body_with_safety(ctx, cid, state);
                out.push(run.pretty(body, ctx));
            }
            NodeKind::Image(run) => {
                let body = build_inline_body_with_safety(ctx, cid, state);
                out.push(run.pretty(body, ctx));
            }
            NodeKind::Autolink(run) => {
                out.push(run.pretty());
                state.note_content();
            }
            NodeKind::FootnoteReference(r) => {
                out.push(r.pretty());
                state.note_content();
            }
            NodeKind::Math(span) => {
                out.push(span.pretty(ctx, &node.raw_range));
                state.note_content();
            }
            NodeKind::TaskListMarker(_) => {}
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
                    "non-inline NodeKind reached pretty_paragraph_inline: {:?}",
                    &node.kind
                );
                out.push(text(ctx.tree.raw_text(ctx.source, cid).to_owned()));
                state.note_content();
            }
        }
        *left_emphasis_delim = None;
    }
}

/// Recurse into the inline children of a wrapper (emphasis, strong,
/// link, image, strikethrough) with the same paragraph-safety state.
/// Returns the assembled body as a single `Doc`.
fn build_inline_body_with_safety<'a>(ctx: &PrettyCtx<'a>, parent: NodeId, state: &mut ParagraphSafetyState) -> Doc<'a> {
    let ids: Vec<NodeId> = ctx.tree.children(parent).collect();
    let mut parts: Vec<Doc<'a>> = Vec::with_capacity(ids.len());
    let mut left_emphasis_delim: Option<EmphasisDelim> = None;
    walk_paragraph_inline(ctx, &ids, &mut parts, state, &mut left_emphasis_delim);
    concat(parts)
}

/// Emit a single Run with paragraph-safety escapes applied to its
/// text fragments. `has_next_sibling` is true iff this run is
/// followed by another inline node in the parent — used to decide
/// `LineContext::MoreContent` vs `EndOfLine` for the last text in
/// the run.
fn emit_run_with_safety(
    run: &InlineRun,
    has_next_sibling: bool,
    out: &mut Vec<Doc<'_>>,
    state: &mut ParagraphSafetyState,
) {
    let parts = run.parts();
    for (i, part) in parts.iter().enumerate() {
        match part {
            RunPart::Text(s) if s.is_empty() => {}
            RunPart::Text(s) => {
                let next = next_line_context(parts, i, has_next_sibling);
                let escaped = if state.at_line_start {
                    escape_for_block_start(s.as_str(), next)
                } else if state.after_break && state.prev_line_had_text {
                    escape_for_paragraph_interrupt(s.as_str(), next)
                        .or_else(|| escape_setext_underline(s.as_str(), next))
                } else if state.after_break {
                    escape_for_block_start(s.as_str(), next)
                } else {
                    None
                };
                // After safety has applied any required escapes, the
                // payload is safe to tokenise word-by-word for the
                // wrap pass — every word starts with a benign byte
                // (the escape, if any, was inserted at byte 0). The
                // resulting `Doc::SoftSpace` boundaries are break
                // candidates only under `Wrap::At(n)`; under `Keep`
                // and `No` they render as literal spaces, preserving
                // source line shape.
                let payload = escaped.unwrap_or_else(|| s.clone());
                out.push(prose(&payload));
                state.note_content();
            }
            RunPart::SoftBreak => {
                out.push(line());
                state.note_soft_break();
            }
            RunPart::HardLineBreak => {
                out.push(concat([text("\\"), hard_line()]));
                state.note_hard_line();
            }
            RunPart::HardBreakTag => {
                out.push(text("<br/>"));
                state.note_content();
            }
        }
    }
}

/// `LineContext` for the text fragment at index `i` of `parts`.
/// Mirrors the old `next_on_same_line` helper but operates on the
/// `RunPart` stream: a following `SoftBreak` / `HardLineBreak` /
/// `HardBreakTag` terminates the line; anything else (more text
/// inside this run, or a following inline sibling outside it) is
/// more content on the same line.
fn next_line_context(parts: &[RunPart], i: usize, has_next_sibling: bool) -> LineContext {
    match parts.get(i.saturating_add(1)) {
        Some(RunPart::SoftBreak | RunPart::HardLineBreak | RunPart::HardBreakTag) => LineContext::EndOfLine,
        Some(RunPart::Text(_)) => LineContext::MoreContent,
        None => {
            if has_next_sibling {
                LineContext::MoreContent
            } else {
                LineContext::EndOfLine
            }
        }
    }
}

/// Drop any leading or trailing `Doc::Line` / `Doc::HardLine` from the
/// assembled paragraph body. Pulldown can emit a trailing `SoftBreak`
/// when a paragraph's last content line is followed by a
/// whitespace-only line that the parser elides (e.g. form-feed
/// content). Without trimming, the break renders as an extra `\n`
/// before the block's own terminator, producing a blank-line drift
/// between formats.
fn trim_edge_breaks(parts: &mut Vec<Doc<'_>>) {
    while matches!(parts.first(), Some(Doc::Line | Doc::HardLine)) {
        parts.remove(0);
    }
    while matches!(parts.last(), Some(Doc::Line | Doc::HardLine)) {
        parts.pop();
    }
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

/// Source bytes immediately before / after the node's `raw_range`,
/// truncated to a single codepoint each side. Pulldown's emphasis-
/// flanking rule (CM §6.2) only inspects one character on each side
/// of a delimiter run, so a single neighbouring codepoint is enough
/// to reach the same decision.
fn flank_ctx_for<'a>(ctx: &PrettyCtx<'a>, id: NodeId) -> FlankCtx<'a> {
    let Some(node) = ctx.tree.node(id) else {
        return FlankCtx::default();
    };
    let range = &node.raw_range;
    let left = preceding_codepoint(ctx.source, range.start);
    let right = following_codepoint(ctx.source, range.end);
    FlankCtx { left, right }
}

/// `&source[..start]`'s last codepoint, if any. Returns `None` at
/// document start.
fn preceding_codepoint(source: &str, start: usize) -> Option<&str> {
    let prefix = source.get(..start)?;
    let mut iter = prefix.char_indices();
    iter.next_back().map(|(i, _)| &prefix[i..])
}

/// `&source[end..]`'s first codepoint, if any. Returns `None` at
/// document end.
fn following_codepoint(source: &str, end: usize) -> Option<&str> {
    let suffix = source.get(end..)?;
    let mut iter = suffix.char_indices();
    let first = iter.next()?;
    let next_offset = iter.next().map_or(suffix.len(), |(i, _)| i);
    Some(&suffix[first.0..next_offset])
}
