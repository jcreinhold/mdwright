//! Tree IR for structural document shape.
//!
//! The [`crate::ir::Ir`] flat IR is enough for `scan-and-emit-diagnostic`
//! lint rules, but it does not retain container nesting: a paragraph
//! that lives three list items deep inside a blockquote looks identical
//! to a top-level paragraph in the flat IR. The tree IR is an owned arena of
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
//! `Ir::parse` collects `pulldown_cmark::Parser` events once. The
//! flat IR records lint-facing slices, then the tree builder consumes
//! the same events after math regions are known.

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, CowStr, Event, LinkType, Tag};

use crate::cm::math::MathRegion;
use crate::cm::math::span::MathSpan;
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
}

/// One node in the document tree. A pure data carrier; behaviour
/// lives in dedicated modules.
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
        /// `Some` when the source carried an ATX `{#id .class key=val}`
        /// trailer (pulldown `Tag::Heading::id|classes|attrs` non-empty);
        /// `None` otherwise. Boxed so the enum's largest-variant size
        /// is unaffected by the rare attribute case.
        attrs: Option<Box<crate::cm::block::heading::HeadingAttrs>>,
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
    /// Container for a definition list (pulldown `Tag::DefinitionList`,
    /// enabled by `Options::ENABLE_DEFINITION_LIST`). Direct children
    /// are alternating [`DefinitionTerm`](NodeKind::DefinitionTerm) and
    /// [`DefinitionDescription`](NodeKind::DefinitionDescription) nodes
    /// in source order.
    DefinitionList,
    /// One term in a [`DefinitionList`](NodeKind::DefinitionList).
    /// Children are inline.
    DefinitionTerm,
    /// One definition body in a [`DefinitionList`](NodeKind::DefinitionList).
    /// Children are block-level (paragraphs, lists, code blocks, …).
    DefinitionDescription,
    // Inline:
    /// A coalesced run of text + soft/hard breaks.
    Run,
    /// One inline code span.
    CodeRun,
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        /// Raw reference label for reference-style links; `None` for
        /// inline links. Used only to downgrade unresolved references
        /// to raw-source nodes after the reference table is built.
        reference_label: Option<String>,
    },
    Image {
        /// Raw reference label for reference-style images; `None` for
        /// inline images.
        reference_label: Option<String>,
    },
    Autolink,
    /// One inline HTML span.
    HtmlSpan,
    FootnoteReference,
    TaskListMarker(bool),
    /// One recognised math region.
    ///
    /// The `Box` is a deliberate layout choice: `MathSpan` is the
    /// largest inline payload (≈56 B; `MathBody` carries a sorted
    /// `Box<[Range<usize>]>` of transparent runs), and inlining it
    /// here would grow every `Node` by the same margin even though
    /// most trees have zero or one math leaf. Math leaves are rare
    /// enough that the per-leaf heap allocation is not a hot-path
    /// cost.
    Math(Box<MathSpan>),

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
/// The builder records source ranges for structural nodes. It buffers
/// `Event::Text` / `Event::SoftBreak` / `Event::HardBreak` into one
/// `NodeKind::Run` until a non-text event ends the run. Block code and
/// block HTML accumulate their bodies on the open frame so the closing
/// event can stamp the container with a `body` field.
pub(crate) struct TreeBuilder<'a> {
    source: &'a str,
    arena: Vec<Node>,
    child_ids: Vec<NodeId>,
    parents: Vec<Option<NodeId>>,
    /// Scratch buffer; the tail beyond `open.last().pending_start` is
    /// the current open frame's accumulated children.
    pending: Vec<NodeId>,
    open: Vec<OpenFrame>,
    /// Source byte range covered by buffered inline events.
    inline_range: Option<Range<usize>>,
    /// Recognised math regions, sorted by `range.start`. The text
    /// handler splices a [`NodeKind::Math`] leaf at each region and
    /// drops subsequent pulldown events whose bytes were already
    /// covered by the emitted leaf.
    math_regions: &'a [MathRegion],
    /// Monotonic index into [`Self::math_regions`]; advances past
    /// regions whose end is before the next event we process.
    math_cursor: usize,
    /// Exclusive end byte of the most recently emitted math leaf.
    /// Any pulldown event whose range ends at or before this byte was
    /// already covered by the leaf and must be swallowed.
    math_emitted_until: usize,
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
    pub(crate) fn new(source: &'a str, math_regions: &'a [MathRegion]) -> Self {
        // Allocate the Document root at index 0 up front, then open
        // a frame for it; `finalize` closes the frame.
        let root = Node {
            kind: NodeKind::Document,
            raw_range: 0..source.len(),
            children: 0..0,
            subtree_end: 1,
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
            inline_range: None,
            math_regions,
            math_cursor: 0,
            math_emitted_until: 0,
        }
    }

    /// Number of nodes allocated in the arena so far. Used only by
    /// `Ir::parse` for trace output; not part of the builder's
    /// formal interface.
    pub(crate) fn arena_len(&self) -> usize {
        self.arena.len()
    }

    #[allow(clippy::wildcard_enum_match_arm)]
    pub(crate) fn handle(&mut self, event: &Event<'a>, range: Range<usize>) {
        // Math-overlay swallow guard. Any event whose bytes were
        // already covered by a previously-emitted `NodeKind::Math`
        // leaf is dropped — its content is part of the math region's
        // outer range and will be rendered via `MathSpan::pretty`.
        // Multi-line display math (`$$\nx=1\n$$`) emits several
        // `Event::Text` / `Event::SoftBreak` events whose ranges all
        // fall inside the math region; this guard drops them after
        // the first text event has emitted the leaf.
        if self.math_emitted_until > range.start && range.end <= self.math_emitted_until {
            return;
        }
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
                    NodeKind::CodeBlock { fenced: false, .. } => widen_to_line_start(self.source, range),
                    NodeKind::HtmlBlock { .. } => widen_to_line_start_through_ws(self.source, range),
                    _ => range,
                };
                let body_accum =
                    matches!(&kind, NodeKind::CodeBlock { .. } | NodeKind::HtmlBlock { .. }).then(String::new);
                self.open_container(kind, range, body_accum);
            }
            Event::End(end) => {
                self.flush_inline_run();
                self.close_container(range);
                let _ = end;
            }
            Event::Text(cow) => {
                if let Some(buf) = self.body_accum_mut() {
                    buf.push_str(cow);
                    return;
                }
                // If the event starts inside an already-emitted math
                // region (because pulldown emitted the `\)` closer as
                // a decoded `)` and `extend_for_backslash` would pull
                // the range back into the leaf), trim the leading
                // in-math portion and re-derive the trailing prose
                // from source bytes — the decoded `cow` doesn't align
                // with the trimmed range.
                if range.start < self.math_emitted_until {
                    let trimmed = self.math_emitted_until..range.end;
                    if trimmed.is_empty() {
                        return;
                    }
                    if self.text_overlaps_math(&trimmed) {
                        self.splice_text_with_math(trimmed);
                    } else {
                        self.push_source_prose(trimmed);
                    }
                    return;
                }
                let raw_range = self.extend_for_backslash(range);
                if self.text_overlaps_math(&raw_range) {
                    self.splice_text_with_math(raw_range);
                } else {
                    let _ = cow;
                    self.push_inline_text(raw_range);
                }
            }
            Event::Code(cow) => {
                self.flush_inline_run();
                let _ = cow;
                self.push_leaf(NodeKind::CodeRun, range);
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
                let _ = cow;
                self.push_leaf(NodeKind::HtmlSpan, range);
            }
            Event::InlineHtml(cow) => {
                self.flush_inline_run();
                let _ = cow;
                self.push_leaf(NodeKind::HtmlSpan, range);
            }
            Event::FootnoteReference(label) => {
                self.flush_inline_run();
                let _ = label;
                self.push_leaf(NodeKind::FootnoteReference, range);
            }
            Event::SoftBreak => {
                self.push_inline_break(range);
            }
            Event::HardBreak => {
                self.push_inline_break(range);
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
                self.push_leaf(NodeKind::TaskListMarker(*checked), range);
            }
            // Math is not enabled in Options; if it ever appears,
            // record the bytes inline as text.
            Event::InlineMath(cow) | Event::DisplayMath(cow) => {
                let raw_range = range;
                let _ = cow;
                self.push_inline_text(raw_range);
            }
        }
    }

    /// Push source text into the inline accumulator.
    fn push_inline_text(&mut self, range: Range<usize>) {
        self.extend_inline_range(&range);
    }

    /// `true` iff any math region overlaps `range`. Advances
    /// `math_cursor` past regions entirely before `range.start` as a
    /// side effect so the answer is `O(matching regions)` amortised.
    fn text_overlaps_math(&mut self, range: &Range<usize>) -> bool {
        while self.math_cursor < self.math_regions.len()
            && self
                .math_regions
                .get(self.math_cursor)
                .is_some_and(|r| r.range.end <= range.start)
        {
            self.math_cursor = self.math_cursor.saturating_add(1);
        }
        self.math_regions
            .get(self.math_cursor)
            .is_some_and(|r| r.range.start < range.end)
    }

    /// Splice `Event::Text` whose `raw_range` overlaps one or more
    /// math regions. Prose chunks outside math become
    /// source-byte runs; each math region flushes the current inline
    /// run and emits a `NodeKind::Math` leaf. Pulldown's decoded
    /// payload is dropped for the overlapping event because (a) for
    /// dollar-form math the source bytes equal the decoded payload
    /// (no decoding happens for `$`), and (b) for backslash-delimited
    /// math (`\(` / `\[`) pulldown emits the decoded delimiter as a
    /// separate, single-character event whose bytes lie fully inside
    /// the math region — that event is dropped by the swallow guard
    /// at the top of `handle`, never reaching this splice.
    ///
    /// `extend_for_backslash` can pull an event's start back to the
    /// `\` byte of a delimiter pulldown already consumed (e.g., the
    /// `\)` closing a `\(…\)` inline math); the cursor is clamped to
    /// `math_emitted_until` and regions already past are skipped via
    /// `math_cursor` so the leaf is not re-emitted.
    fn splice_text_with_math(&mut self, raw_range: Range<usize>) {
        let mut cursor = raw_range.start.max(self.math_emitted_until);
        while self.math_cursor < self.math_regions.len() {
            let Some(region) = self.math_regions.get(self.math_cursor) else {
                break;
            };
            if region.range.start >= raw_range.end {
                break;
            }
            if region.range.end <= cursor {
                self.math_cursor = self.math_cursor.saturating_add(1);
                continue;
            }
            if cursor < region.range.start {
                let chunk = cursor..region.range.start;
                self.push_source_prose(chunk);
            }
            self.flush_inline_run();
            let span = region.span.clone();
            let region_range = region.range.clone();
            tracing::trace!(?region_range, "math leaf");
            self.push_leaf(NodeKind::Math(Box::new(span)), region_range.clone());
            self.math_emitted_until = region_range.end;
            cursor = region_range.end;
            self.math_cursor = self.math_cursor.saturating_add(1);
        }
        if cursor < raw_range.end {
            self.push_source_prose(cursor..raw_range.end);
        }
    }

    /// Push a prose chunk drawn directly from source bytes. Used by
    /// the math splice for the prose segments around math regions.
    fn push_source_prose(&mut self, range: Range<usize>) {
        self.push_inline_text(range);
    }

    /// Push a break event into the inline accumulator.
    fn push_inline_break(&mut self, range: Range<usize>) {
        self.extend_inline_range(&range);
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

    /// Flush any buffered inline events into a [`NodeKind::Run`] leaf
    /// under the current open frame. No-op when the buffer is empty.
    fn flush_inline_run(&mut self) {
        if let Some(range) = self.inline_range.take()
            && !range.is_empty()
        {
            self.push_leaf(NodeKind::Run, range);
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
        });
        let parent = self.open.last().map(|f| f.arena_id);
        self.parents.push(parent);
        // Stake this node as a child of the currently-open frame.
        self.pending.push(id);
        id
    }

    fn open_container(&mut self, kind: NodeKind, range: Range<usize>, body_accum: Option<String>) {
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
        let children_start = u32::try_from(self.child_ids.len()).unwrap_or(u32::MAX);
        self.child_ids.extend(self.pending.drain(pending_start..));
        let children_end = u32::try_from(self.child_ids.len()).unwrap_or(u32::MAX);
        let subtree_end = u32::try_from(self.arena.len()).unwrap_or(u32::MAX);

        let raw_range = frame.raw_start..range.end;
        let node_is_list = matches!(
            self.arena.get(frame.arena_id.idx()).map(|n| &n.kind),
            Some(NodeKind::List { .. })
        );

        // Stamp children / subtree_end / raw_range / body first so
        // the arena reflects the final structure before deriving list
        // tightness from direct item children.
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

        if node_is_list {
            let list_tight = list_has_no_direct_paragraph_items(&self.arena, &self.child_ids, frame.arena_id);
            if let Some(node) = self.arena.get_mut(frame.arena_id.idx())
                && let NodeKind::List { tight, .. } = &mut node.kind
            {
                *tight = list_tight;
            }
        }
    }

    fn push_leaf(&mut self, kind: NodeKind, range: Range<usize>) {
        self.alloc_node(kind, range);
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
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => {
                let lvl = *level as u32;
                // ATX headings start with `#` after optional leading
                // space; setext headings start with the heading text.
                let setext = first_non_whitespace_byte(self.source, range.start) != Some(b'#');
                let parsed = build_heading_attrs(self.source, range, id.as_deref(), classes, attrs);
                NodeKind::Heading {
                    level: lvl,
                    setext,
                    attrs: parsed.map(Box::new),
                }
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
            Tag::HtmlBlock => NodeKind::HtmlBlock { body: String::new() },
            Tag::List(start) => NodeKind::List {
                ordered: start.is_some(),
                start: start.unwrap_or(0),
                tight: true,
                marker_byte: derive_list_marker_byte(self.source, range.clone(), start.is_some()).unwrap_or(0),
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
            Tag::Emphasis => NodeKind::Emphasis,
            Tag::Strong => NodeKind::Strong,
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
            Tag::DefinitionList => NodeKind::DefinitionList,
            Tag::DefinitionListTitle => NodeKind::DefinitionTerm,
            Tag::DefinitionListDefinition => NodeKind::DefinitionDescription,
            Tag::MetadataBlock(_) => NodeKind::Unknown { tag: "MetadataBlock" },
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
            NodeKind::Link { reference_label } => (reference_label.as_deref(), false),
            NodeKind::Image { reference_label } => (reference_label.as_deref(), true),
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

fn link_kind(lt: LinkType, dest_url: &CowStr<'_>, title: &CowStr<'_>, id: &CowStr<'_>, is_image: bool) -> NodeKind {
    let _ = (dest_url, title);
    let reference_label = match lt {
        LinkType::Autolink => {
            return NodeKind::Autolink;
        }
        LinkType::Email => {
            return NodeKind::Autolink;
        }
        LinkType::WikiLink { .. } => return NodeKind::Unknown { tag: "WikiLink" },
        LinkType::Inline => None,
        LinkType::Reference
        | LinkType::ReferenceUnknown
        | LinkType::Collapsed
        | LinkType::CollapsedUnknown
        | LinkType::Shortcut
        | LinkType::ShortcutUnknown => Some(id.to_string()),
    };
    if is_image {
        NodeKind::Image { reference_label }
    } else {
        NodeKind::Link { reference_label }
    }
}

fn first_non_whitespace_byte(source: &str, start: usize) -> Option<u8> {
    source
        .as_bytes()
        .get(start..)?
        .iter()
        .copied()
        .find(|b| !matches!(b, b' ' | b'\t'))
}

/// Find the *list marker byte* anywhere in the source range.
///
/// `Tag::List`'s reported range can include a parent container's
/// marker bytes when the inner separator is a tab: `>\t-` makes
/// pulldown emit a List with range `0..3`, so a naive
/// "first non-whitespace byte from `range.start`" scan returns `>`,
/// not `-` (`fuzz_blockquote_tab_list_marker.in`). Scanning for the
/// first byte matching the *legal* marker set is unambiguous because
/// pulldown already decided this is a list, and parent container
/// prefixes (`>`, `|`) are never list markers.
fn derive_list_marker_byte(source: &str, range: Range<usize>, ordered: bool) -> Option<u8> {
    source.as_bytes().get(range)?.iter().copied().find(|b| {
        if ordered {
            b.is_ascii_digit()
        } else {
            matches!(b, b'-' | b'*' | b'+')
        }
    })
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

/// Build a [`HeadingAttrs`] from the parsed pulldown fields when the
/// heading carried a `{ #id .class key=val }` trailer. Returns `None`
/// when all three field slots are empty (the no-trailer case).
///
/// The `source_trailer` field is recovered from the heading's source
/// bytes via [`crate::cm::block::heading::find_attr_trailer_range`];
/// it powers [`crate::config::HeadingAttrsStyle::Preserve`] emission
/// (verbatim trailer round-trip).
fn build_heading_attrs(
    source: &str,
    range: &Range<usize>,
    id: Option<&str>,
    classes: &[CowStr<'_>],
    attrs: &[(CowStr<'_>, Option<CowStr<'_>>)],
) -> Option<crate::cm::block::heading::HeadingAttrs> {
    if id.is_none() && classes.is_empty() && attrs.is_empty() {
        return None;
    }
    let raw = source.get(range.clone()).unwrap_or("");
    let trailer = crate::cm::block::heading::find_attr_trailer_range(raw)
        .and_then(|r| raw.get(r))
        .unwrap_or("")
        .to_owned();
    Some(crate::cm::block::heading::HeadingAttrs {
        id: id.map(str::to_owned),
        classes: classes.iter().map(|c| c.to_string()).collect(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.as_ref().map(|v| v.to_string())))
            .collect(),
        source_trailer: trailer,
    })
}

fn list_has_no_direct_paragraph_items(arena: &[Node], child_ids: &[NodeId], list_id: NodeId) -> bool {
    let Some(list_node) = arena.get(list_id.idx()) else {
        return true;
    };
    for i in list_node.children.clone() {
        let Some(&item_id) = child_ids.get(i as usize) else {
            continue;
        };
        let Some(item_node) = arena.get(item_id.idx()) else {
            continue;
        };
        if !matches!(item_node.kind, NodeKind::Item { .. }) {
            continue;
        }
        if item_has_direct_paragraph(arena, child_ids, item_node) {
            return false;
        }
    }
    true
}

fn item_has_direct_paragraph(arena: &[Node], child_ids: &[NodeId], item: &Node) -> bool {
    for j in item.children.clone() {
        let Some(&cid) = child_ids.get(j as usize) else {
            continue;
        };
        if matches!(arena.get(cid.idx()).map(|n| &n.kind), Some(NodeKind::Paragraph)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::Ir;

    #[test]
    fn empty_doc_has_root_only() {
        let ir = Ir::parse_str("");
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
        let ir = Ir::parse_str("Hello world\n");
        let tree = &ir.tree;
        let kinds: Vec<&NodeKind> = tree
            .descendants(tree.root())
            .filter_map(|id| tree.node(id).map(|n| &n.kind))
            .collect();
        assert!(kinds.iter().any(|k| matches!(k, NodeKind::Paragraph)));
        assert!(kinds.iter().any(|k| matches!(k, NodeKind::Run)));
    }

    #[test]
    fn raw_ranges_are_well_formed() {
        let src = "# Title\n\nA paragraph.\n\n- one\n- two\n";
        let ir = Ir::parse_str(src);
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
        let ir = Ir::parse_str(src);
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
                        raw.trim_end_matches('\n').ends_with("```") || raw.trim_end_matches('\n').ends_with("~~~"),
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
        let ir = Ir::parse_str(src);
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
                parent.raw_range.start <= child.raw_range.start && child.raw_range.end <= parent.raw_range.end,
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
        let ir = Ir::parse_str("> a quote\n");
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
        let ir = Ir::parse_str("- one\n- two\n");
        assert_eq!(find_list_tight(&ir.tree), Some(true));
    }

    #[test]
    fn loose_list_with_blank_line_between_items() {
        let ir = Ir::parse_str("- one\n\n- two\n");
        assert_eq!(find_list_tight(&ir.tree), Some(false));
    }

    #[test]
    fn nested_blockquote_under_list() {
        let ir = Ir::parse_str("- item\n\n  > quote\n");
        let tree = &ir.tree;
        let bq = tree
            .descendants(tree.root())
            .find(|&id| matches!(tree.node(id).map(|n| &n.kind), Some(NodeKind::BlockQuote)));
        assert!(bq.is_some(), "blockquote nested under list item");
    }

    #[test]
    fn reference_link_records_label() {
        let src = "[foo][bar]\n\n[bar]: https://example.com\n";
        let ir = Ir::parse_str(src);
        let tree = &ir.tree;
        let label = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Link { reference_label }) => reference_label.clone(),
                _ => None,
            })
            .expect("link present");
        assert_eq!(label, "bar");
    }

    #[test]
    fn link_reference_definitions_appear_in_reference_table() {
        // Defs only enter the table when at least one reference uses
        // them (the new pulldown-event-driven resolver dropped the
        // "emit unused defs verbatim" behaviour because unused defs
        // never affect HTML output anyway).
        let src = "[a]: https://a.example\n[b]: https://b.example\n\n[a] and [b].\n";
        let ir = Ir::parse_str(src);
        let mut labels: Vec<String> = ir.refs.iter().map(|t| t.label_raw.clone()).collect();
        labels.sort();
        assert_eq!(labels, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn autolink_preserves_url() {
        let ir = Ir::parse_str("<https://example.com>\n");
        let tree = &ir.tree;
        let has_autolink = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Autolink) => Some(true),
                _ => None,
            })
            .expect("autolink present");
        assert!(has_autolink);
    }

    #[test]
    fn task_list_marker_sets_item_task() {
        let ir = Ir::parse_str("- [x] done\n- [ ] todo\n");
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
        let ir = Ir::parse_str("```rust\nfn x() {}\n```\n");
        let tree = &ir.tree;
        let info = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::CodeBlock { fenced: true, info, .. }) => Some(info.clone()),
                _ => None,
            })
            .expect("fenced code block");
        assert_eq!(info, "rust");
    }
}
