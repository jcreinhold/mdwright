//! Tree IR — the formatter's input.
//!
//! The [`crate::ir::Ir`] flat IR is enough for `scan-and-emit-diagnostic`
//! lint rules but cannot drive a pretty-printer: a paragraph that lives
//! three list items deep inside a blockquote looks identical to a top-
//! level paragraph in the flat IR. The tree IR is an owned arena of
//! [`Node`] values rooted at a Document node, built during the *same*
//! pulldown-cmark event walk as the flat IR — two IRs, one parse pass.
//!
//! ## Storage
//!
//! `Tree` holds three vectors:
//!
//! - `arena: Vec<Node>` in pre-order DFS. Each node carries
//!   `subtree_end` so `descendants(id)` is the contiguous range
//!   `arena[id+1 .. subtree_end]` — no recursion, no allocation.
//! - `child_ids: Vec<NodeId>` — a parent's *direct* children are not
//!   contiguous in the arena (between them sit each child's whole
//!   subtree), so direct children are stored separately. Each node's
//!   `children: Range<u32>` indexes into this table.
//! - `parents: Vec<Option<NodeId>>` — parallel to `arena` for O(1)
//!   parent lookup without bloating `Node`.
//!
//! ## Two IRs, one walk
//!
//! `Ir::parse` walks `pulldown_cmark::Parser` once. Each event is
//! handed first to the [`TreeBuilder`] (by reference) and then to the
//! flat [`crate::ir::Builder`] (by value). The linter continues to use
//! the flat IR; the formatter (sessions 06+) consumes
//! [`crate::Document::tree`].

use std::borrow::Cow;
use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, CowStr, Event, LinkType, Tag};

use crate::cm::block::TypedBlock;
use crate::cm::block::code::{CodeFenceChar, FencedCodeBlock, IndentedCodeBlock};
use crate::cm::block::footnote::FootnoteDef;
use crate::cm::block::heading::{Heading, HeadingLevel, HeadingStyle};
use crate::cm::block::html::HtmlBlock;
use crate::cm::block::list::{
    ListBlock, ListItem, ListItemKind, ListMarker, TaskItem, Tightness, item_indent,
};
use crate::cm::block::paragraph::Paragraph;
use crate::cm::block::quote::BlockQuote;
use crate::cm::block::table::{TableBlock, TableCell, TableRow};
use crate::cm::block::thematic::ThematicBreak;
use crate::cm::inline::autolink::AutolinkRun;
use crate::cm::inline::code::InlineCodeRun;
use crate::cm::inline::emphasis::{EmphasisRun, StrongRun};
use crate::cm::inline::escape_policy::EscapeScope;
use crate::cm::inline::footnote::FootnoteReference;
use crate::cm::inline::html::InlineHtmlSpan;
use crate::cm::inline::link::{ImageRun, LinkRun, LinkSourceKind};
use crate::cm::inline::run::{InlineRun, RunInput};
use crate::cm::inline::task::TaskMarker;
use crate::cm::refs::ReferenceTable;

/// Index into [`Tree`]'s arena. Stable for the life of the tree;
/// can only be obtained from `Tree` methods.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    #[must_use]
    pub(crate) fn idx(self) -> usize {
        self.0 as usize
    }

    /// Build a `NodeId` from a raw arena index. Used by unit tests in
    /// sibling modules that need a `NodeId` without standing up a
    /// whole `TreeBuilder`.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn from_index(i: u32) -> Self {
        Self(i)
    }
}

/// One node in the document tree. A pure data carrier — behaviour
/// (pretty-printing, linting) lives in dedicated modules.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub raw_range: Range<usize>,
    /// Range into the owning [`Tree`]'s child-id table. Iterate via
    /// [`Tree::children`]; the field is exposed crate-internally so
    /// the builder can fill it after seeing the matching End event.
    pub(crate) children: Range<u32>,
    /// Exclusive end of this node's subtree in the arena. Always
    /// `>= self_id + 1`; equals `self_id + 1` for leaves.
    pub(crate) subtree_end: u32,
    /// Typed block payload, populated by [`TreeBuilder`] for the
    /// block kinds Phase R has lifted into [`TypedBlock`]. `None`
    /// for inline nodes, for block kinds not yet typed, and for the
    /// rare case where the source-derived data is malformed (e.g., a
    /// `Heading` with level > 6 from a degenerate event stream): the
    /// legacy `NodeKind` data still drives emission in those cases.
    pub(crate) typed: Option<TypedBlock>,
}

/// All node kinds we recognise.
///
/// Container kinds (`Paragraph`, `Heading`, `BlockQuote`, `List`,
/// `Item`, …) may carry direct children. Leaf kinds (`Text`, `Code`,
/// `SoftBreak`, …)
/// never do. The [`Unknown`](NodeKind::Unknown) variant is a forward-
/// compatibility fallback for pulldown-cmark tags we don't model;
/// the formatter falls back to verbatim source emission via
/// [`Tree::raw_text`] for those.
#[derive(Clone, Debug)]
pub enum NodeKind {
    /// The document root. Always at `NodeId(0)`.
    Document,
    Paragraph,
    Heading {
        level: u32,
        /// `true` for setext (`Foo\n===`); `false` for ATX (`# Foo`).
        setext: bool,
    },
    BlockQuote,
    List {
        ordered: bool,
        /// Start index for ordered lists; `0` for unordered.
        start: u64,
        /// `true` iff no direct `Item` child contains a direct
        /// [`Paragraph`](NodeKind::Paragraph) child. Computed at End.
        tight: bool,
        /// Marker byte of the *first* item (`-`, `*`, `+`, or the
        /// first digit of an ordered marker). `0` if the list has no
        /// items (a degenerate but parseable case).
        marker_byte: u8,
    },
    Item {
        /// `Some(checked)` for task list items; `None` otherwise.
        /// Set when a [`TaskListMarker`](NodeKind::TaskListMarker)
        /// child is seen inside the item.
        task: Option<bool>,
    },
    CodeBlock {
        fenced: bool,
        info: String,
        /// Body bytes the parser emitted inside this block. Pulldown
        /// has already stripped any enclosing container's prefix
        /// (list-continuation indent, blockquote `>` markers,
        /// indented-code-block 4-space prefix), so this is the
        /// minimum representation that survives re-nesting.
        body: String,
    },
    HtmlBlock {
        /// Body bytes the parser emitted inside this block. Same
        /// prefix-stripped form as [`CodeBlock::body`].
        body: String,
    },
    ThematicBreak,
    Table {
        alignments: Vec<TableAlign>,
    },
    TableHead,
    TableRow,
    TableCell,
    FootnoteDefinition {
        label: String,
    },
    // Inline:
    /// A coalesced run of text + soft/hard breaks, with the
    /// `CommonMark` escape policy applied at construction.
    Run(InlineRun),
    /// One inline code span.
    CodeRun(InlineCodeRun),
    Emphasis(EmphasisRun),
    Strong(StrongRun),
    Strikethrough,
    Link(LinkRun),
    Image(ImageRun),
    Autolink(AutolinkRun),
    /// One inline HTML span.
    HtmlSpan(InlineHtmlSpan),
    FootnoteReference(FootnoteReference),
    TaskListMarker(TaskMarker),

    /// Forward-compatibility fallback. Pulldown-cmark may emit tags
    /// we don't recognise (math when enabled, definition lists,
    /// super/subscript, wiki links, metadata blocks). Rather than
    /// panicking, the builder records an `Unknown` node with the raw
    /// range; the formatter falls back to byte-verbatim emission.
    Unknown {
        tag: &'static str,
    },
}

/// Column alignment for a [`Table`](NodeKind::Table).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

impl From<Alignment> for TableAlign {
    fn from(a: Alignment) -> Self {
        match a {
            Alignment::None => Self::None,
            Alignment::Left => Self::Left,
            Alignment::Center => Self::Center,
            Alignment::Right => Self::Right,
        }
    }
}

/// An owned arena of [`Node`] values rooted at a Document node.
#[derive(Debug)]
pub struct Tree {
    arena: Vec<Node>,
    child_ids: Vec<NodeId>,
    parents: Vec<Option<NodeId>>,
}

impl Tree {
    /// The Document root. Always present.
    #[must_use]
    #[allow(clippy::unused_self)]
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Look up a node by id. Returns `None` for ids that did not come
    /// from this tree.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.arena.get(id.idx())
    }

    /// Source bytes covered by `id`. Empty string for ids that did not
    /// come from this tree; otherwise always a valid slice.
    ///
    /// Caller passes the canonical source the tree was parsed from.
    #[must_use]
    pub fn raw_text<'a>(&self, source: &'a str, id: NodeId) -> &'a str {
        self.node(id)
            .and_then(|n| source.get(n.raw_range.clone()))
            .unwrap_or("")
    }

    /// Direct children of `id` in source order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        let range = self.node(id).map_or(0..0, |n| n.children.clone());
        Children { tree: self, range }
    }

    /// Parent of `id`, or `None` for the root and for unknown ids.
    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parents.get(id.idx()).copied().flatten()
    }

    /// Every descendant of `id` in pre-order (excluding `id` itself).
    pub fn descendants(&self, id: NodeId) -> Descendants<'_> {
        let start = id.idx().saturating_add(1);
        let end = self.node(id).map_or(start, |n| n.subtree_end as usize);
        Descendants {
            tree: self,
            next: start as u32,
            end: end as u32,
        }
    }

    /// Number of nodes in the tree. Includes the Document root.
    #[must_use]
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// `true` iff the tree has only the Document root.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.arena.len() <= 1
    }
}

/// Iterator over a node's direct children. Returned by
/// [`Tree::children`].
pub struct Children<'t> {
    tree: &'t Tree,
    range: Range<u32>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.range.next()?;
        self.tree.child_ids.get(i as usize).copied()
    }
}

/// Iterator over a node's descendants in pre-order. Returned by
/// [`Tree::descendants`].
pub struct Descendants<'t> {
    tree: &'t Tree,
    next: u32,
    end: u32,
}

impl Iterator for Descendants<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            return None;
        }
        let id = NodeId(self.next);
        // Need to advance, but the caller may not have all nodes
        // built yet only inside the builder; outside, `arena` is
        // complete. `tree.node` returning Some guarantees the
        // `subtree_end` is valid.
        let _ = self.tree.node(id)?;
        self.next = self.next.saturating_add(1);
        Some(id)
    }
}

/// Walks the pulldown-cmark event stream and accumulates an arena
/// tree. Lives inside [`crate::ir::Ir::parse`] alongside the flat-IR
/// builder.
///
/// The builder is the deep module that produces typed inline values:
/// it buffers `Event::Text` / `Event::SoftBreak` / `Event::HardBreak`
/// into an [`InlineRun`] until a non-text event ends the run, then
/// flushes the buffered events through [`InlineRun::new`] with the
/// current escape scope. Code spans and inline HTML are similarly
/// pushed as typed leaves at their event boundaries. Block code and
/// block HTML accumulate their bodies on the open frame so the
/// closing event can stamp the container with a `body` field.
pub(crate) struct TreeBuilder<'a> {
    source: &'a str,
    arena: Vec<Node>,
    child_ids: Vec<NodeId>,
    parents: Vec<Option<NodeId>>,
    /// Scratch buffer; the tail beyond `open.last().pending_start` is
    /// the current open frame's accumulated children.
    pending: Vec<NodeId>,
    open: Vec<OpenFrame>,
    /// Active escape scope for inline children of the currently-open
    /// container. The stack mirrors `open`: push on every `Start`,
    /// pop on every matching `End`. The bottom-most entry is the
    /// document's default scope.
    scope_stack: Vec<EscapeScope>,
    /// Pulldown text/break events buffered for the next [`InlineRun`]
    /// flush.
    inline_buf: Vec<RunInput<'a>>,
    /// Source byte range covered by the buffered inline events;
    /// `None` when `inline_buf` is empty.
    inline_range: Option<Range<usize>>,
}

#[derive(Debug)]
struct OpenFrame {
    arena_id: NodeId,
    pending_start: u32,
    raw_start: usize,
    /// `Some` for `CodeBlock` and `HtmlBlock`: the parser's text /
    /// html payloads inside these containers append to this buffer
    /// instead of going through the inline accumulator. The closing
    /// event stamps the buffer onto the container's `body` field.
    body_accum: Option<String>,
}

impl<'a> TreeBuilder<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        // Allocate the Document root at index 0 up front, then open
        // a frame for it; `finalize` closes the frame.
        let root = Node {
            kind: NodeKind::Document,
            raw_range: 0..source.len(),
            children: 0..0,
            subtree_end: 1,
            typed: None,
        };
        Self {
            source,
            arena: vec![root],
            child_ids: Vec::new(),
            parents: vec![None],
            pending: Vec::new(),
            open: vec![OpenFrame {
                arena_id: NodeId(0),
                pending_start: 0,
                raw_start: 0,
                body_accum: None,
            }],
            scope_stack: vec![EscapeScope::default()],
            inline_buf: Vec::new(),
            inline_range: None,
        }
    }

    #[allow(clippy::wildcard_enum_match_arm)]
    pub(crate) fn handle(&mut self, event: &Event<'a>, range: Range<usize>) {
        match event {
            Event::Start(tag) => {
                self.flush_inline_run();
                let kind = self.kind_for_start(tag, &range);
                // Pulldown's event range for an indented code block
                // starts at the first content byte (after the 4-space
                // / tab prefix). Walk back to the start of the line
                // so `raw_text` includes the indent — the block's
                // identity depends on it.
                //
                // HtmlBlock gets the same widening (limited to
                // space/tab so we don't engulf preceding content)
                // because pulldown's HTML render of `  <?` includes
                // the leading whitespace as text inside the block
                // container; emitting only the post-whitespace bytes
                // drops content that pulldown re-renders identically
                // when present. Fuzz-found
                // `html-pi-leading-whitespace.in`.
                let range = match &kind {
                    NodeKind::CodeBlock { fenced: false, .. } => {
                        widen_to_line_start(self.source, range)
                    }
                    NodeKind::HtmlBlock { .. } => {
                        widen_to_line_start_through_ws(self.source, range)
                    }
                    _ => range,
                };
                let body_accum = matches!(
                    &kind,
                    NodeKind::CodeBlock { .. } | NodeKind::HtmlBlock { .. }
                )
                .then(String::new);
                let scope = self.current_scope_after_start(tag);
                self.open_container(kind, range, body_accum);
                self.scope_stack.push(scope);
            }
            Event::End(end) => {
                self.flush_inline_run();
                self.close_container(range);
                // The scope_stack push in Start matches an End here,
                // even for tags we did not adjust the scope for.
                let _ = end;
                if self.scope_stack.len() > 1 {
                    let _ = self.scope_stack.pop();
                }
            }
            Event::Text(cow) => {
                if let Some(buf) = self.body_accum_mut() {
                    buf.push_str(cow);
                    return;
                }
                let raw_range = self.extend_for_backslash(range);
                let src = self.source.get(raw_range.clone());
                self.push_inline_text(cow_to_cow(cow), src, raw_range);
            }
            Event::Code(cow) => {
                self.flush_inline_run();
                let code = InlineCodeRun::new(cow.as_ref(), self.current_scope());
                self.push_leaf(NodeKind::CodeRun(code), range);
            }
            Event::Html(cow) => {
                if let Some(buf) = self.body_accum_mut() {
                    buf.push_str(cow);
                    return;
                }
                // Defensive: a block-level Html event outside an
                // HtmlBlock container. Treat it as inline HTML so the
                // bytes survive verbatim.
                self.flush_inline_run();
                let span = InlineHtmlSpan::from_parser(cow.as_ref(), range.start, self.source);
                self.push_leaf(NodeKind::HtmlSpan(span), range);
            }
            Event::InlineHtml(cow) => {
                self.flush_inline_run();
                let span = InlineHtmlSpan::from_parser(cow.as_ref(), range.start, self.source);
                self.push_leaf(NodeKind::HtmlSpan(span), range);
            }
            Event::FootnoteReference(label) => {
                self.flush_inline_run();
                let r = FootnoteReference::new(label.to_string());
                self.push_leaf(NodeKind::FootnoteReference(r), range);
            }
            Event::SoftBreak => {
                self.push_inline_break(RunInput::SoftBreak, range);
            }
            Event::HardBreak => {
                self.push_inline_break(RunInput::HardBreak, range);
            }
            Event::Rule => {
                self.flush_inline_run();
                self.push_leaf(NodeKind::ThematicBreak, range);
            }
            Event::TaskListMarker(checked) => {
                self.flush_inline_run();
                if let Some(frame) = self.open.last()
                    && let Some(node) = self.arena.get_mut(frame.arena_id.idx())
                    && let NodeKind::Item { ref mut task } = node.kind
                {
                    *task = Some(*checked);
                }
                self.push_leaf(NodeKind::TaskListMarker(TaskMarker::new(*checked)), range);
            }
            // Math is not enabled in Options; if it ever appears,
            // record the bytes inline as text.
            Event::InlineMath(cow) | Event::DisplayMath(cow) => {
                let raw_range = range;
                let src = self.source.get(raw_range.clone());
                self.push_inline_text(cow_to_cow(cow), src, raw_range);
            }
        }
    }

    /// Push a decoded text payload into the inline accumulator.
    fn push_inline_text(
        &mut self,
        payload: Cow<'a, str>,
        source: Option<&'a str>,
        range: Range<usize>,
    ) {
        self.extend_inline_range(&range);
        self.inline_buf.push(RunInput::Text { payload, source });
    }

    /// Push a break event into the inline accumulator.
    fn push_inline_break(&mut self, brk: RunInput<'a>, range: Range<usize>) {
        self.extend_inline_range(&range);
        self.inline_buf.push(brk);
    }

    fn extend_inline_range(&mut self, range: &Range<usize>) {
        match &mut self.inline_range {
            Some(r) => {
                if range.start < r.start {
                    r.start = range.start;
                }
                if range.end > r.end {
                    r.end = range.end;
                }
            }
            None => self.inline_range = Some(range.clone()),
        }
    }

    /// Flush any buffered inline events into an [`InlineRun`] leaf
    /// under the current open frame. No-op when the buffer is empty.
    fn flush_inline_run(&mut self) {
        if self.inline_buf.is_empty() {
            self.inline_range = None;
            return;
        }
        let inputs = std::mem::take(&mut self.inline_buf);
        let range = self.inline_range.take().unwrap_or(0..0);
        let scope = self.current_scope();
        let run = InlineRun::new(inputs, scope);
        if !run.is_empty() {
            self.push_leaf(NodeKind::Run(run), range);
        }
    }

    fn current_scope(&self) -> EscapeScope {
        self.scope_stack.last().copied().unwrap_or_default()
    }

    /// Scope active for inline children of the container opened by
    /// `tag`. Defaults to the parent scope; specific tags layer their
    /// flag on top.
    #[allow(clippy::wildcard_enum_match_arm)]
    fn current_scope_after_start(&self, tag: &Tag<'a>) -> EscapeScope {
        let parent = self.current_scope();
        match tag {
            Tag::Heading { .. } => EscapeScope {
                in_heading: true,
                ..parent
            },
            Tag::Link { .. } | Tag::Image { .. } => EscapeScope {
                in_link_text: true,
                ..parent
            },
            Tag::TableCell => EscapeScope {
                in_table_cell: true,
                ..parent
            },
            _ => parent,
        }
    }

    /// `Some(&mut String)` when the current open frame is a
    /// `CodeBlock` or `HtmlBlock` accumulating body bytes.
    fn body_accum_mut(&mut self) -> Option<&mut String> {
        self.open.last_mut().and_then(|f| f.body_accum.as_mut())
    }

    /// Downgrade unresolved reference-style links to raw source
    /// emission, then seal the Document root. Reference definitions
    /// themselves live in [`ReferenceTable`] (pulldown-cmark 0.13
    /// does not emit events for them); the formatter reads that table
    /// directly rather than via synthesised tree children.
    #[tracing::instrument(level = "debug", skip(self, refs))]
    pub(crate) fn finalize(mut self, refs: &ReferenceTable) -> Tree {
        // Flush any inline events left in the buffer (the document's
        // trailing run before the parser exhausted its events).
        self.flush_inline_run();
        // The Document frame is still open. Close it. `new` always
        // pushed exactly one frame, so this pop must succeed; if it
        // ever doesn't, fall through with no Document children.
        let doc_pending_start = self.open.pop().map_or(0u32, |f| f.pending_start);
        let doc_children: Vec<NodeId> = self.pending.drain(doc_pending_start as usize..).collect();

        // Validate every reference-style Link / Image node against the
        // table; unresolvable references downgrade to `Unknown` so the
        // formatter emits the original source span verbatim (CM §4.7
        // "leave as text").
        downgrade_unresolved_links(&mut self.arena, refs);

        let children_start = u32::try_from(self.child_ids.len()).unwrap_or(u32::MAX);
        self.child_ids.extend(doc_children.iter().copied());
        let children_end = u32::try_from(self.child_ids.len()).unwrap_or(u32::MAX);
        let subtree_end = u32::try_from(self.arena.len()).unwrap_or(u32::MAX);

        if let Some(root) = self.arena.get_mut(0) {
            root.children = children_start..children_end;
            root.subtree_end = subtree_end;
            root.raw_range = 0..self.source.len();
        }

        Tree {
            arena: self.arena,
            child_ids: self.child_ids,
            parents: self.parents,
        }
    }

    fn alloc_node(&mut self, kind: NodeKind, raw_range: Range<usize>) -> NodeId {
        let id = NodeId(u32::try_from(self.arena.len()).unwrap_or(u32::MAX));
        let subtree_end = id.0.saturating_add(1);
        self.arena.push(Node {
            kind,
            raw_range,
            children: 0..0,
            subtree_end,
            typed: None,
        });
        let parent = self.open.last().map(|f| f.arena_id);
        self.parents.push(parent);
        // Stake this node as a child of the currently-open frame.
        self.pending.push(id);
        id
    }

    fn open_container(
        &mut self,
        kind: NodeKind,
        range: Range<usize>,
        body_accum: Option<String>,
    ) {
        let raw_start = range.start;
        let id = self.alloc_node(kind, range);
        let pending_start = u32::try_from(self.pending.len()).unwrap_or(u32::MAX);
        self.open.push(OpenFrame {
            arena_id: id,
            pending_start,
            raw_start,
            body_accum,
        });
    }

    fn close_container(&mut self, range: Range<usize>) {
        let Some(frame) = self.open.pop() else {
            return;
        };
        // Drain this frame's direct children out of `pending` and
        // record them contiguously in `child_ids`.
        let pending_start = frame.pending_start as usize;
        let drained_len = self.pending.len().saturating_sub(pending_start);
        let children_start = u32::try_from(self.child_ids.len()).unwrap_or(u32::MAX);
        self.child_ids.extend(self.pending.drain(pending_start..));
        let children_end = u32::try_from(self.child_ids.len()).unwrap_or(u32::MAX);
        let subtree_end = u32::try_from(self.arena.len()).unwrap_or(u32::MAX);

        let _ = drained_len;
        let raw_range = frame.raw_start..range.end;
        let node_is_list = matches!(
            self.arena.get(frame.arena_id.idx()).map(|n| &n.kind),
            Some(NodeKind::List { .. })
        );

        // Stamp children / subtree_end / raw_range / body first so
        // the arena reflects the final structure before we build the
        // typed view (which reads the arena immutably).
        if let Some(node) = self.arena.get_mut(frame.arena_id.idx()) {
            node.children = children_start..children_end;
            node.subtree_end = subtree_end;
            node.raw_range = raw_range;
            // Stamp the accumulated body onto CodeBlock / HtmlBlock.
            #[allow(clippy::wildcard_enum_match_arm)]
            if let Some(body) = frame.body_accum {
                match &mut node.kind {
                    NodeKind::CodeBlock { body: dst, .. } => *dst = body,
                    NodeKind::HtmlBlock { body: dst } => *dst = body,
                    _ => {}
                }
            }
        }

        // Build the typed-block view. Lists and tables need a
        // structural walk of their children so they go through
        // dedicated builders; other kinds project from `NodeKind`
        // alone. A `None` typed value means either the kind is not
        // yet typed or the source-derived data violated an invariant
        // — in which case the legacy `kind` still drives emission.
        let node_is_table = matches!(
            self.arena.get(frame.arena_id.idx()).map(|n| &n.kind),
            Some(NodeKind::Table { .. })
        );
        let typed = if node_is_list {
            build_list_block(&self.arena, &self.child_ids, self.source, frame.arena_id)
                .map(TypedBlock::ListBlock)
        } else if node_is_table {
            build_table_block(&self.arena, &self.child_ids, frame.arena_id).map(TypedBlock::Table)
        } else {
            self.arena
                .get(frame.arena_id.idx())
                .and_then(|n| build_typed_block(&n.kind, self.source, n.raw_range.clone()))
        };

        if let Some(node) = self.arena.get_mut(frame.arena_id.idx()) {
            // Mirror derived tightness into the legacy `NodeKind::List`
            // field so the legacy formatter (block.rs:794) keeps
            // working unchanged. This mirror retires alongside the
            // formatter swap in prompt 27.
            if let (NodeKind::List { tight: t, .. }, Some(TypedBlock::ListBlock(lb))) =
                (&mut node.kind, &typed)
            {
                *t = matches!(lb.tightness(), Tightness::Tight);
            }
            node.typed = typed;
        }
    }

    /// Handle the tail portion of an event whose source range
    fn push_leaf(&mut self, kind: NodeKind, range: Range<usize>) {
        let id = self.alloc_node(kind, range);
        // Stamp the typed view for the leaf block kinds we model
        // (currently just `ThematicBreak`); the inline leaves keep
        // their typed payload inside their `NodeKind` variant.
        if let Some(node) = self.arena.get_mut(id.idx()) {
            node.typed = build_typed_block(&node.kind, self.source, node.raw_range.clone());
        }
    }

    /// Reclaim a leading `\` that pulldown-cmark consumed as a
    /// backslash escape. Mirrors `crate::ir::Builder::push_prose`.
    fn extend_for_backslash(&self, range: Range<usize>) -> Range<usize> {
        if range.start > 0 {
            let bytes = self.source.as_bytes();
            if bytes.get(range.start.saturating_sub(1)) == Some(&b'\\') {
                return range.start.saturating_sub(1)..range.end;
            }
        }
        range
    }

    fn kind_for_start(&self, tag: &Tag<'a>, range: &Range<usize>) -> NodeKind {
        match tag {
            Tag::Paragraph => NodeKind::Paragraph,
            Tag::Heading { level, .. } => {
                let lvl = *level as u32;
                // ATX headings start with `#` after optional leading
                // space; setext headings start with the heading text.
                let setext = first_non_whitespace_byte(self.source, range.start) != Some(b'#');
                NodeKind::Heading { level: lvl, setext }
            }
            Tag::BlockQuote(_) => NodeKind::BlockQuote,
            Tag::CodeBlock(kind) => {
                let (fenced, info) = match kind {
                    CodeBlockKind::Fenced(s) => (true, s.to_string()),
                    CodeBlockKind::Indented => (false, String::new()),
                };
                NodeKind::CodeBlock {
                    fenced,
                    info,
                    body: String::new(),
                }
            }
            Tag::HtmlBlock => NodeKind::HtmlBlock {
                body: String::new(),
            },
            Tag::List(start) => NodeKind::List {
                ordered: start.is_some(),
                start: start.unwrap_or(0),
                tight: true,
                marker_byte: first_non_whitespace_byte(self.source, range.start).unwrap_or(0),
            },
            Tag::Item => NodeKind::Item { task: None },
            Tag::FootnoteDefinition(label) => NodeKind::FootnoteDefinition {
                label: label.to_string(),
            },
            Tag::Table(aligns) => NodeKind::Table {
                alignments: aligns.iter().copied().map(TableAlign::from).collect(),
            },
            Tag::TableHead => NodeKind::TableHead,
            Tag::TableRow => NodeKind::TableRow,
            Tag::TableCell => NodeKind::TableCell,
            Tag::Emphasis => {
                let source_delim = emphasis_open_byte(self.source, range);
                NodeKind::Emphasis(EmphasisRun::from_source(source_delim))
            }
            Tag::Strong => {
                let source_delim = emphasis_open_byte(self.source, range);
                NodeKind::Strong(StrongRun::from_source(source_delim))
            }
            Tag::Strikethrough => NodeKind::Strikethrough,
            Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            } => link_kind(*link_type, dest_url, title, id, /* is_image= */ false),
            Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            } => link_kind(*link_type, dest_url, title, id, /* is_image= */ true),
            Tag::Superscript => NodeKind::Unknown { tag: "Superscript" },
            Tag::Subscript => NodeKind::Unknown { tag: "Subscript" },
            Tag::DefinitionList => NodeKind::Unknown {
                tag: "DefinitionList",
            },
            Tag::DefinitionListTitle => NodeKind::Unknown {
                tag: "DefinitionListTitle",
            },
            Tag::DefinitionListDefinition => NodeKind::Unknown {
                tag: "DefinitionListDefinition",
            },
            Tag::MetadataBlock(_) => NodeKind::Unknown {
                tag: "MetadataBlock",
            },
        }
    }
}

/// Replace every reference-style [`NodeKind::Link`] / [`NodeKind::Image`]
/// whose label fails to resolve against `refs` with [`NodeKind::Unknown`].
/// `Unknown` is the formatter's "emit verbatim source" fallback, which is
/// exactly the behaviour CM §4.7 prescribes for an unresolvable reference.
#[allow(clippy::wildcard_enum_match_arm)] // many irrelevant NodeKind variants
fn downgrade_unresolved_links(arena: &mut [Node], refs: &ReferenceTable) {
    for node in arena.iter_mut() {
        let (label_opt, is_image): (Option<&str>, bool) = match &node.kind {
            NodeKind::Link(run) => (run.reference_label(), false),
            NodeKind::Image(run) => (run.reference_label(), true),
            _ => (None, false),
        };
        let Some(label) = label_opt else { continue };
        if refs.resolve(label).is_some() {
            continue;
        }
        let tag = if is_image { "Image" } else { "Link" };
        node.kind = NodeKind::Unknown { tag };
        // Drop the subtree's structural children: `Unknown` is emitted
        // verbatim from `raw_text`, so the children must not be
        // rendered separately. Clearing the children range is enough —
        // the arena entries linger but become unreachable from the
        // root.
        node.children = 0..0;
    }
}

fn link_kind(
    lt: LinkType,
    dest_url: &CowStr<'_>,
    title: &CowStr<'_>,
    id: &CowStr<'_>,
    is_image: bool,
) -> NodeKind {
    let ref_kind = match lt {
        LinkType::Autolink => {
            return NodeKind::Autolink(AutolinkRun::new(dest_url.to_string()));
        }
        LinkType::Email => {
            return NodeKind::Autolink(AutolinkRun::new(dest_url.to_string()));
        }
        LinkType::WikiLink { .. } => return NodeKind::Unknown { tag: "WikiLink" },
        LinkType::Inline => None,
        LinkType::Reference | LinkType::ReferenceUnknown => Some(LinkSourceKind::ReferenceFull),
        LinkType::Collapsed | LinkType::CollapsedUnknown => {
            Some(LinkSourceKind::ReferenceCollapsed)
        }
        LinkType::Shortcut | LinkType::ShortcutUnknown => Some(LinkSourceKind::ReferenceShortcut),
    };
    let dest = dest_url.to_string();
    let title = title.to_string();
    match ref_kind {
        None => {
            if is_image {
                NodeKind::Image(ImageRun::from_pulldown_inline(dest, title))
            } else {
                NodeKind::Link(LinkRun::from_pulldown_inline(dest, title))
            }
        }
        Some(kind) => {
            let label = id.to_string();
            if is_image {
                NodeKind::Image(ImageRun::from_pulldown_reference(kind, dest, title, label))
            } else {
                NodeKind::Link(LinkRun::from_pulldown_reference(kind, dest, title, label))
            }
        }
    }
}

/// Convert a pulldown `CowStr` into a standard `Cow<str>` without
/// copying when the input is borrowed. Kept for the inline-text path
/// where the borrow into the original event payload is preserved
/// through the run buffer.
fn cow_to_cow<'a>(s: &CowStr<'a>) -> Cow<'a, str> {
    match s {
        CowStr::Borrowed(b) => Cow::Borrowed(b),
        CowStr::Boxed(b) => Cow::Owned(b.to_string()),
        CowStr::Inlined(i) => Cow::Owned(i.to_string()),
    }
}

/// First `*` or `_` byte inside `range` — the opening delimiter of an
/// Emphasis / Strong node. Falls back to `b'*'` if the range is empty
/// or contains no delimiter byte (defensive; pulldown should always
/// open such an event on one of the two).
fn emphasis_open_byte(source: &str, range: &Range<usize>) -> u8 {
    source
        .as_bytes()
        .get(range.clone())
        .into_iter()
        .flatten()
        .copied()
        .find(|b| *b == b'*' || *b == b'_')
        .unwrap_or(b'*')
}

fn first_non_whitespace_byte(source: &str, start: usize) -> Option<u8> {
    source
        .as_bytes()
        .get(start..)?
        .iter()
        .copied()
        .find(|b| !matches!(b, b' ' | b'\t'))
}

/// Widen `range` so its start sits at the beginning of the line that
/// contains `range.start`. Used by [`Builder::handle`] for indented
/// code blocks: pulldown's event range starts at the first content
/// byte, which loses the 4-space / tab prefix the block's identity
/// depends on.
fn widen_to_line_start(source: &str, range: Range<usize>) -> Range<usize> {
    let bytes = source.as_bytes();
    let mut start = range.start.min(bytes.len());
    while start > 0 && bytes.get(start.saturating_sub(1)).copied() != Some(b'\n') {
        start = start.saturating_sub(1);
    }
    start..range.end
}

/// Like [`widen_to_line_start`] but only consumes ASCII space / tab
/// bytes. Stops at any other content (a non-whitespace byte before
/// the line start means the range is genuinely mid-line and should
/// not be extended). Used for `HtmlBlock` whose CM §4.6 opener allows
/// 0–3 spaces of leading indent that pulldown's HTML render includes
/// as part of the block.
fn widen_to_line_start_through_ws(source: &str, range: Range<usize>) -> Range<usize> {
    let bytes = source.as_bytes();
    let mut start = range.start.min(bytes.len());
    while start > 0 {
        match bytes.get(start.saturating_sub(1)).copied() {
            Some(b' ' | b'\t') => start = start.saturating_sub(1),
            Some(b'\n') | None => break,
            Some(_) => return range, // non-whitespace before line start: don't widen
        }
    }
    start..range.end
}

/// Project a [`NodeKind`] onto its [`TypedBlock`] view, if one exists.
///
/// Returns `None` when the kind is inline, is a block kind Phase R has
/// not yet typed (paragraph, list, table, HTML block, footnote def,
/// link-ref def), or when the source-derived data violates the typed
/// constructor's invariant — in which case the legacy `NodeKind` still
/// drives emission. The typed value's existence is a witness that the
/// data round-trips under the relevant `CommonMark` §4 rule.
#[allow(clippy::wildcard_enum_match_arm)]
fn build_typed_block(
    kind: &NodeKind,
    source: &str,
    raw_range: Range<usize>,
) -> Option<TypedBlock> {
    use crate::config::ThematicStyle;
    match kind {
        NodeKind::Heading { level, setext } => {
            let lvl = u8::try_from(*level).ok()?;
            let level = HeadingLevel::try_new(lvl).ok()?;
            let style = if *setext {
                HeadingStyle::Setext
            } else {
                HeadingStyle::Atx
            };
            Heading::try_new(level, style).ok().map(TypedBlock::Heading)
        }
        NodeKind::CodeBlock {
            fenced: true,
            info,
            body,
        } => {
            let char = source_fence_char(source, raw_range).unwrap_or(CodeFenceChar::Backtick);
            Some(TypedBlock::FencedCodeBlock(FencedCodeBlock::new(
                char,
                info.clone(),
                body.clone(),
            )))
        }
        NodeKind::CodeBlock {
            fenced: false,
            body,
            ..
        } => Some(TypedBlock::IndentedCodeBlock(IndentedCodeBlock::new(
            body.clone(),
        ))),
        NodeKind::BlockQuote => Some(TypedBlock::BlockQuote(BlockQuote::new())),
        NodeKind::ThematicBreak => {
            // The chosen style is a formatter policy, not a tree-IR
            // fact; stamp the prompt-16 default here, and let the
            // emitter swap in `FmtOptions::thematic_break_style` at
            // render time.
            Some(TypedBlock::ThematicBreak(ThematicBreak::new(
                ThematicStyle::Dash,
            )))
        }
        NodeKind::Paragraph => Some(TypedBlock::Paragraph(Paragraph::new())),
        NodeKind::HtmlBlock { body } => Some(TypedBlock::HtmlBlock(HtmlBlock::new(body.clone()))),
        NodeKind::FootnoteDefinition { label } => {
            Some(TypedBlock::FootnoteDef(FootnoteDef::new(label.clone())))
        }
        _ => None,
    }
}

/// Build the typed [`ListBlock`] view from a list `Node`'s arena
/// state. Returns `None` for degenerate shapes (no items, marker byte
/// outside `-*+0..9`); the IR falls back to legacy `NodeKind::List`
/// emission in that case.
fn build_list_block(
    arena: &[Node],
    child_ids: &[NodeId],
    source: &str,
    list_id: NodeId,
) -> Option<ListBlock> {
    let list_node = arena.get(list_id.idx())?;
    let NodeKind::List {
        ordered,
        start,
        marker_byte,
        ..
    } = &list_node.kind
    else {
        return None;
    };
    let marker = ListMarker::from_legacy(
        *ordered,
        *start,
        *marker_byte,
        source,
        list_node.raw_range.clone(),
    )?;

    let mut items: Vec<ListItemKind> = Vec::new();
    for i in list_node.children.clone() {
        let Some(&item_id) = child_ids.get(i as usize) else {
            continue;
        };
        let Some(item_node) = arena.get(item_id.idx()) else {
            continue;
        };
        let NodeKind::Item { task: task_state } = item_node.kind else {
            continue;
        };
        let indent = item_indent(source, item_node.raw_range.clone());
        let has_para = item_has_direct_paragraph(arena, child_ids, item_node);
        items.push(match task_state {
            Some(checked) => {
                let body_empty = task_item_body_empty(arena, child_ids, item_node);
                ListItemKind::Task(TaskItem::new(
                    item_id, indent, has_para, checked, body_empty,
                ))
            }
            None => ListItemKind::Plain(ListItem::new(item_id, indent, has_para)),
        });
    }
    ListBlock::try_new(marker, items).ok()
}

/// Build the typed [`TableBlock`] view from a `Table` node's arena
/// state. Walks the head row and each body row, runs each through
/// [`TableRow::from_raw`] for GFM §4.10 column-count reconciliation,
/// and hands the result to [`TableBlock::try_new`]. Returns `None`
/// only when the alignment vector is empty or the arena lookup
/// fails — pulldown-cmark does not produce such tables from valid
/// input. The legacy `NodeKind::Table` keeps driving emission until
/// prompt 27's printer swap.
fn build_table_block(
    arena: &[Node],
    child_ids: &[NodeId],
    table_id: NodeId,
) -> Option<TableBlock> {
    let table_node = arena.get(table_id.idx())?;
    let NodeKind::Table { alignments } = &table_node.kind else {
        return None;
    };
    let expected = alignments.len();

    let mut head: Option<TableRow> = None;
    let mut body: Vec<TableRow> = Vec::new();

    for i in table_node.children.clone() {
        let Some(&child_id) = child_ids.get(i as usize) else {
            continue;
        };
        let Some(child_node) = arena.get(child_id.idx()) else {
            continue;
        };
        #[allow(clippy::wildcard_enum_match_arm)]
        match &child_node.kind {
            NodeKind::TableHead => {
                let cells = collect_row_cells(arena, child_ids, child_node);
                head = Some(TableRow::from_raw(child_id, cells, expected));
            }
            NodeKind::TableRow => {
                let cells = collect_row_cells(arena, child_ids, child_node);
                body.push(TableRow::from_raw(child_id, cells, expected));
            }
            _ => {}
        }
    }

    // A table with no head is a degenerate pulldown shape — leave
    // the typed view absent so the legacy formatter handles it.
    let head = head?;
    TableBlock::try_new(alignments.clone(), head, body).ok()
}

fn collect_row_cells(arena: &[Node], child_ids: &[NodeId], row: &Node) -> Vec<TableCell> {
    let mut cells = Vec::new();
    for j in row.children.clone() {
        let Some(&cid) = child_ids.get(j as usize) else {
            continue;
        };
        if matches!(
            arena.get(cid.idx()).map(|n| &n.kind),
            Some(NodeKind::TableCell)
        ) {
            cells.push(TableCell::new(cid));
        }
    }
    cells
}

fn item_has_direct_paragraph(arena: &[Node], child_ids: &[NodeId], item: &Node) -> bool {
    for j in item.children.clone() {
        let Some(&cid) = child_ids.get(j as usize) else {
            continue;
        };
        if matches!(
            arena.get(cid.idx()).map(|n| &n.kind),
            Some(NodeKind::Paragraph)
        ) {
            return true;
        }
    }
    false
}

/// A task item's body is empty when its only inline content is the
/// `TaskListMarker` leaf. Pulldown nests the marker inside the item's
/// first paragraph, so we inspect both the item's direct children and
/// the grandchildren of any direct `Paragraph`.
fn task_item_body_empty(arena: &[Node], child_ids: &[NodeId], item: &Node) -> bool {
    for j in item.children.clone() {
        let Some(&cid) = child_ids.get(j as usize) else {
            continue;
        };
        let Some(child) = arena.get(cid.idx()) else {
            continue;
        };
        if matches!(child.kind, NodeKind::TaskListMarker(_)) {
            continue;
        }
        if matches!(child.kind, NodeKind::Paragraph) {
            for k in child.children.clone() {
                let Some(&gcid) = child_ids.get(k as usize) else {
                    continue;
                };
                let Some(gchild) = arena.get(gcid.idx()) else {
                    continue;
                };
                if !matches!(gchild.kind, NodeKind::TaskListMarker(_)) {
                    return false;
                }
            }
        } else {
            return false;
        }
    }
    true
}

/// First fence character (`` ` `` or `~`) inside `raw_range`. Used to
/// reconstruct a [`FencedCodeBlock`]'s fence type from the source —
/// pulldown's [`CodeBlockKind::Fenced`] carries only the info string.
fn source_fence_char(source: &str, raw_range: Range<usize>) -> Option<CodeFenceChar> {
    let bytes = source.as_bytes().get(raw_range)?;
    bytes
        .iter()
        .copied()
        .find(|b| !matches!(*b, b' ' | b'\t' | b'\n' | b'\r'))
        .and_then(|b| match b {
            b'`' => Some(CodeFenceChar::Backtick),
            b'~' => Some(CodeFenceChar::Tilde),
            _ => None,
        })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::Ir;

    #[test]
    fn empty_doc_has_root_only() {
        let ir = Ir::parse("");
        let tree = &ir.tree;
        assert_eq!(tree.root(), NodeId(0));
        assert!(tree.is_empty());
        assert!(matches!(
            tree.node(tree.root()).map(|n| &n.kind),
            Some(NodeKind::Document)
        ));
    }

    #[test]
    fn paragraph_and_text_present() {
        let ir = Ir::parse("Hello world\n");
        let tree = &ir.tree;
        let kinds: Vec<&NodeKind> = tree
            .descendants(tree.root())
            .filter_map(|id| tree.node(id).map(|n| &n.kind))
            .collect();
        assert!(kinds.iter().any(|k| matches!(k, NodeKind::Paragraph)));
        assert!(kinds.iter().any(|k| matches!(k, NodeKind::Run(_))));
    }

    #[test]
    fn raw_ranges_are_well_formed() {
        let src = "# Title\n\nA paragraph.\n\n- one\n- two\n";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        for id in tree.descendants(tree.root()) {
            let n = tree.node(id).expect("descendants only yields valid ids");
            assert!(n.raw_range.start <= n.raw_range.end);
            assert!(n.raw_range.end <= src.len());
        }
    }

    #[test]
    fn raw_range_covers_leading_sigil_per_block_kind() {
        // Verbatim emission relies on `raw_text(id)` containing every
        // block's full lexical extent — including the opening sigil
        // (`#`, `>`, `-`, fence markers, indented-code prefix). A
        // regression that drops the sigil would cause verbatim
        // emission to silently lose information.
        let src = "\
# Heading
> quote
- list item
```rust
let x = 1;
```
    indented
---
<!-- html block -->
";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        for id in tree.descendants(tree.root()) {
            let n = tree.node(id).expect("descendants yields valid ids");
            let raw = tree.raw_text(src, id);
            #[allow(clippy::wildcard_enum_match_arm)]
            match &n.kind {
                NodeKind::Heading { setext: false, .. } => {
                    assert!(raw.starts_with('#'), "ATX heading missing `#`: {raw:?}");
                }
                NodeKind::BlockQuote => {
                    assert!(raw.starts_with('>'), "blockquote missing `>`: {raw:?}");
                }
                NodeKind::List { .. } => {
                    let first = raw.bytes().next().expect("non-empty list raw_text");
                    assert!(
                        matches!(first, b'-' | b'*' | b'+' | b'0'..=b'9'),
                        "list missing bullet: {raw:?}",
                    );
                }
                NodeKind::CodeBlock { fenced: true, .. } => {
                    assert!(
                        raw.starts_with("```") || raw.starts_with("~~~"),
                        "fenced code block missing opening fence: {raw:?}",
                    );
                    assert!(
                        raw.trim_end_matches('\n').ends_with("```")
                            || raw.trim_end_matches('\n').ends_with("~~~"),
                        "fenced code block missing closing fence: {raw:?}",
                    );
                }
                NodeKind::CodeBlock { fenced: false, .. } => {
                    assert!(
                        raw.starts_with("    ") || raw.starts_with('\t'),
                        "indented code block missing 4-space prefix: {raw:?}",
                    );
                }
                NodeKind::HtmlBlock { .. } => {
                    assert!(raw.starts_with('<'), "HTML block missing `<`: {raw:?}");
                }
                NodeKind::ThematicBreak => {
                    let first = raw.bytes().next().expect("non-empty thematic break");
                    assert!(
                        matches!(first, b'-' | b'*' | b'_'),
                        "thematic break missing marker: {raw:?}",
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn child_raw_range_is_contained_in_parent() {
        // Containment is the structural form of "every node's source
        // bytes lie inside its parent's source bytes". Required for
        // verbatim emission to compose: a parent emitted verbatim
        // wholly covers all of its children, so nested re-emission
        // would double-print but never lose information.
        let src = "# H\n\n> quote with *em*\n\n- item one\n- item two\n";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        for id in tree.descendants(tree.root()) {
            if id == tree.root() {
                continue;
            }
            let Some(parent_id) = tree.parent(id) else {
                continue;
            };
            let child = tree.node(id).expect("valid child id");
            let parent = tree.node(parent_id).expect("valid parent id");
            assert!(
                parent.raw_range.start <= child.raw_range.start
                    && child.raw_range.end <= parent.raw_range.end,
                "child {:?} {:?} outside parent {:?} {:?}",
                child.kind,
                child.raw_range,
                parent.kind,
                parent.raw_range,
            );
        }
    }

    #[test]
    fn parent_chain_terminates_at_root() {
        let ir = Ir::parse("> a quote\n");
        let tree = &ir.tree;
        let last = NodeId(u32::try_from(tree.len().saturating_sub(1)).unwrap_or(0));
        let mut cur = last;
        let mut steps: u32 = 0;
        while let Some(p) = tree.parent(cur) {
            cur = p;
            steps = steps.saturating_add(1);
            assert!(steps < 32, "walk did not terminate");
        }
        assert_eq!(cur, tree.root());
        assert!(tree.parent(tree.root()).is_none());
    }

    fn find_list_tight(tree: &Tree) -> Option<bool> {
        tree.descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::List { tight, .. }) => Some(*tight),
                _ => None,
            })
    }

    #[test]
    fn tight_list_one_text_child() {
        let ir = Ir::parse("- one\n- two\n");
        assert_eq!(find_list_tight(&ir.tree), Some(true));
    }

    #[test]
    fn loose_list_with_blank_line_between_items() {
        let ir = Ir::parse("- one\n\n- two\n");
        assert_eq!(find_list_tight(&ir.tree), Some(false));
    }

    #[test]
    fn nested_blockquote_under_list() {
        let ir = Ir::parse("- item\n\n  > quote\n");
        let tree = &ir.tree;
        let bq = tree
            .descendants(tree.root())
            .find(|&id| matches!(tree.node(id).map(|n| &n.kind), Some(NodeKind::BlockQuote)));
        assert!(bq.is_some(), "blockquote nested under list item");
    }

    #[test]
    fn reference_link_records_label() {
        let src = "[foo][bar]\n\n[bar]: https://example.com\n";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        let link = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Link(run)) => {
                    Some((run.source().kind(), run.label().map(str::to_owned)))
                }
                _ => None,
            })
            .expect("link present");
        assert_eq!(link.0, Some(LinkSourceKind::ReferenceFull));
        assert_eq!(link.1.as_deref(), Some("bar"));
    }

    #[test]
    fn collapsed_reference_link() {
        let src = "[foo][]\n\n[foo]: https://example.com\n";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        let kind = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Link(run)) => Some(run.source().kind()),
                _ => None,
            })
            .expect("link present");
        assert_eq!(kind, Some(LinkSourceKind::ReferenceCollapsed));
    }

    #[test]
    fn shortcut_reference_link() {
        let src = "[foo]\n\n[foo]: https://example.com\n";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        let kind = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Link(run)) => Some(run.source().kind()),
                _ => None,
            })
            .expect("link present");
        assert_eq!(kind, Some(LinkSourceKind::ReferenceShortcut));
    }

    // Load-bearing invariant for prompt 27's total dispatcher: every
    // printable block node carries a typed payload on `Node.typed`,
    // and every printable inline NodeKind variant *is* its typed
    // payload (one-arg tuple variant). The kitchen-sink fixture
    // exercises every printable kind we expect; an unexercised arm
    // means the fixture is missing a construct.
    const TYPED_COVERAGE_KITCHEN: &str =
        include_str!("../tests/fixtures/typed_coverage_kitchen.md");

    fn is_printable_block(k: &NodeKind) -> bool {
        // `Item` and table sub-parts (TableHead/Row/Cell) are not in
        // this set: their typed data lives inside the parent
        // `ListBlock` / `TableBlock` payload, not on `node.typed`.
        matches!(
            k,
            NodeKind::Paragraph
                | NodeKind::Heading { .. }
                | NodeKind::BlockQuote
                | NodeKind::List { .. }
                | NodeKind::CodeBlock { .. }
                | NodeKind::HtmlBlock { .. }
                | NodeKind::ThematicBreak
                | NodeKind::Table { .. }
                | NodeKind::FootnoteDefinition { .. }
        )
    }

    #[test]
    fn every_printable_block_has_some_typed() {
        let ir = Ir::parse(TYPED_COVERAGE_KITCHEN);
        let tree = &ir.tree;
        for id in tree.descendants(tree.root()) {
            let node = tree.node(id).expect("descendant id is valid");
            if is_printable_block(&node.kind) {
                assert!(
                    node.typed.is_some(),
                    "block node {id:?} {:?} has no typed payload",
                    node.kind,
                );
            }
        }
    }

    #[test]
    fn every_printable_inline_carries_typed_payload() {
        // Compile-time check: each printable inline NodeKind variant
        // matches as a single typed payload. Adding a new inline
        // variant without a typed wrapper forces this match to be
        // updated.
        let ir = Ir::parse(TYPED_COVERAGE_KITCHEN);
        let tree = &ir.tree;
        for id in tree.descendants(tree.root()) {
            let node = tree.node(id).expect("descendant id is valid");
            match &node.kind {
                NodeKind::Run(_)
                | NodeKind::CodeRun(_)
                | NodeKind::HtmlSpan(_)
                | NodeKind::Emphasis(_)
                | NodeKind::Strong(_)
                | NodeKind::Strikethrough
                | NodeKind::Link(_)
                | NodeKind::Image(_)
                | NodeKind::Autolink(_)
                | NodeKind::FootnoteReference(_)
                | NodeKind::TaskListMarker(_) => {}
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
                | NodeKind::Unknown { .. } => {}
            }
        }
    }

    #[test]
    fn link_reference_definitions_appear_in_reference_table() {
        // Defs only enter the table when at least one reference uses
        // them (the new pulldown-event-driven resolver dropped the
        // "emit unused defs verbatim" behaviour because unused defs
        // never affect HTML output anyway).
        let src = "[a]: https://a.example\n[b]: https://b.example\n\n[a] and [b].\n";
        let ir = Ir::parse(src);
        let mut labels: Vec<String> = ir.refs.iter().map(|t| t.label_raw().to_owned()).collect();
        labels.sort();
        assert_eq!(labels, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn autolink_preserves_url() {
        let ir = Ir::parse("<https://example.com>\n");
        let tree = &ir.tree;
        let url = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Autolink(run)) => Some(run.url().to_owned()),
                _ => None,
            })
            .expect("autolink present");
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn task_list_marker_sets_item_task() {
        let ir = Ir::parse("- [x] done\n- [ ] todo\n");
        let tree = &ir.tree;
        let items: Vec<Option<bool>> = tree
            .descendants(tree.root())
            .filter_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Item { task }) => Some(*task),
                _ => None,
            })
            .collect();
        assert_eq!(items, vec![Some(true), Some(false)]);
    }

    #[test]
    fn code_block_info_string() {
        let ir = Ir::parse("```rust\nfn x() {}\n```\n");
        let tree = &ir.tree;
        let info = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::CodeBlock {
                    fenced: true, info, ..
                }) => Some(info.clone()),
                _ => None,
            })
            .expect("fenced code block");
        assert_eq!(info, "rust");
    }
}
