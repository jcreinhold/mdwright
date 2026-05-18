//! Definition lists (mdformat-mkdocs / python-markdown extension,
//! pulldown `Tag::DefinitionList` under `Options::ENABLE_DEFINITION_LIST`).
//!
//! The canonical emission shape matches mdformat-mkdocs:
//!
//! ```markdown
//! Term
//! :   Definition body, indented four spaces, possibly
//!     wrapping across multiple lines.
//!
//! Second term
//! :   Another definition.
//! ```
//!
//! Each definition body's continuation lines are indented four spaces
//! (so the body aligns with the column right after `:   `). Multiple
//! definitions for one term are emitted on consecutive `:   ` lines
//! with no blank line between them; a blank line separates term groups.
//!
//! The typed value is a unit struct: every `DefinitionList` shares the
//! same emission rule, and the per-instance variation (term/definition
//! count, body structure) lives in the [`crate::tree::Tree`] arena.

use std::borrow::Cow;

use crate::format::doc::{Doc, LinePrefix, concat, hard_line, prefix_lines, text, unbreakable};
use crate::format::pretty::PrettyCtx;
use crate::tree::{NodeId, NodeKind};

/// Block kinds that can appear as a direct child of a
/// [`NodeKind::DefinitionDescription`]. Anything else flowing through
/// the description child loop is treated as inline content (text /
/// emphasis / strong / link / …) and coalesced into a virtual
/// paragraph, mirroring `cm::block::list::render_item_body`.
fn is_block_kind(kind: Option<&NodeKind>) -> bool {
    matches!(
        kind,
        Some(
            NodeKind::Paragraph
                | NodeKind::Heading { .. }
                | NodeKind::BlockQuote
                | NodeKind::CodeBlock { .. }
                | NodeKind::HtmlBlock { .. }
                | NodeKind::ThematicBreak
                | NodeKind::List { .. }
                | NodeKind::Table { .. }
                | NodeKind::FootnoteDefinition { .. }
                | NodeKind::DefinitionList
        )
    )
}

/// First-line marker for a definition body. The trailing three spaces
/// align the body with the four-space continuation indent.
const TERM_MARKER: &str = ":   ";
/// Continuation-line prefix for the definition body. Four spaces.
const CONT_PREFIX: &str = "    ";

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct DefinitionList;

impl DefinitionList {
    pub(crate) fn new() -> Self {
        Self
    }

    /// Walk the list's direct children. `DefinitionTerm` children emit
    /// their inline content as a line. `DefinitionDescription` children
    /// emit `:   ` followed by their block sequence, with continuation
    /// lines prefixed by four spaces. A blank line precedes any term
    /// whose previous sibling was a description, grouping definitions
    /// visually under their term.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self, clippy::wildcard_enum_match_arm)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        let children: Vec<NodeId> = ctx.tree.children(id).collect();
        let mut parts: Vec<Doc<'a>> = Vec::with_capacity(children.len().saturating_mul(2));
        let mut prev_was_description = false;

        for &cid in &children {
            let Some(node) = ctx.tree.node(cid) else {
                continue;
            };
            match &node.kind {
                NodeKind::DefinitionTerm => {
                    if prev_was_description {
                        parts.push(hard_line());
                    }
                    let inline = crate::format::inline::pretty_inline_children(ctx, cid);
                    parts.push(concat([inline, hard_line()]));
                    prev_was_description = false;
                }
                NodeKind::DefinitionDescription => {
                    // mdformat-mkdocs canonical form is "loose" (blank
                    // line before the `:` marker) when the description
                    // has multiple block children — multi-paragraph,
                    // or a paragraph plus a nested list / code block.
                    // Single-block descriptions stay tight.
                    if description_is_loose(ctx, cid) {
                        parts.push(hard_line());
                    }
                    let body = render_description_body(ctx, cid);
                    let prefixed = prefix_lines(
                        LinePrefix {
                            content: Cow::Borrowed(CONT_PREFIX),
                            blank: Cow::Borrowed(""),
                        },
                        body,
                    );
                    parts.push(concat([unbreakable(text(TERM_MARKER)), prefixed]));
                    prev_was_description = true;
                }
                _ => {}
            }
        }

        concat(parts)
    }
}

/// `true` when a `DefinitionDescription` should be rendered in
/// mdformat-mkdocs "loose" form (blank line between the preceding
/// term and the `:` marker). A description is loose iff it carries
/// more than one block-level child — multi-paragraph, or a paragraph
/// plus a nested list / code block. Single-block descriptions stay
/// tight, matching mdformat-mkdocs canonical.
fn description_is_loose(ctx: &PrettyCtx<'_>, id: NodeId) -> bool {
    let mut block_children = 0usize;
    for cid in ctx.tree.children(id) {
        let kind = ctx.tree.node(cid).map(|n| &n.kind);
        if is_block_kind(kind) {
            block_children = block_children.saturating_add(1);
            if block_children >= 2 {
                return true;
            }
        }
    }
    false
}

/// Build the body Doc for one `DefinitionDescription`. Mirrors
/// `cm::block::list::render_item_body`: groups runs of inline children
/// into virtual paragraphs (so "tight" descriptions — where pulldown
/// emits Text events as direct children with no `Paragraph` wrapper —
/// render as a single paragraph) and recurses into block children
/// normally. Multi-paragraph descriptions get a blank-line separator
/// between block elements via the surrounding `pretty_block_sequence`
/// equivalent.
fn render_description_body<'a>(ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
    let children: Vec<NodeId> = ctx.tree.children(id).collect();
    let mut parts: Vec<Doc<'a>> = Vec::new();
    let mut inline_run: Vec<NodeId> = Vec::new();
    let mut emitted = 0usize;

    let flush_inline = |run: &mut Vec<NodeId>, parts: &mut Vec<Doc<'a>>, emitted: &mut usize| {
        if run.is_empty() {
            return;
        }
        if *emitted > 0 {
            parts.push(hard_line());
        }
        let body = crate::format::inline::pretty_paragraph_inline_for_ids(ctx, run);
        parts.push(concat([body, hard_line()]));
        *emitted = emitted.saturating_add(1);
        run.clear();
    };

    for cid in children {
        let kind = ctx.tree.node(cid).map(|n| &n.kind);
        if is_block_kind(kind) {
            flush_inline(&mut inline_run, &mut parts, &mut emitted);
            if emitted > 0 {
                parts.push(hard_line());
            }
            parts.push(crate::format::block::pretty_block(ctx, cid));
            emitted = emitted.saturating_add(1);
        } else {
            inline_run.push(cid);
        }
    }
    flush_inline(&mut inline_run, &mut parts, &mut emitted);
    if emitted == 0 {
        parts.push(hard_line());
    }
    concat(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_list_is_uniquely_inhabited() {
        assert_eq!(DefinitionList::new(), DefinitionList);
    }
}
