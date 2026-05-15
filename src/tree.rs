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

use crate::ir::LinkDef;

/// Index into [`Tree`]'s arena. Stable for the life of the tree;
/// can only be obtained from `Tree` methods.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    #[must_use]
    fn idx(self) -> usize {
        self.0 as usize
    }
}

/// One node in the document tree. A pure data carrier — behaviour
/// (pretty-printing, linting) lives in dedicated modules.
#[derive(Clone, Debug)]
pub struct Node<'a> {
    pub kind: NodeKind<'a>,
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
pub enum NodeKind<'a> {
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
        info: Cow<'a, str>,
    },
    HtmlBlock,
    ThematicBreak,
    Table {
        alignments: Vec<TableAlign>,
    },
    TableHead,
    TableRow,
    TableCell,
    FootnoteDefinition {
        label: Cow<'a, str>,
    },
    /// Reference link definition (`[label]: dest "title"`). Synthesised
    /// from [`crate::ir::Ir::link_defs`] after the event walk; appears
    /// as a direct child of the Document node in source order.
    LinkReferenceDefinition {
        label: Cow<'a, str>,
        dest: Cow<'a, str>,
        title: Option<Cow<'a, str>>,
    },

    // Inline:
    Text(Cow<'a, str>),
    Code(Cow<'a, str>),
    Emphasis,
    Strong,
    Strikethrough,
    Link {
        dest: Cow<'a, str>,
        title: Cow<'a, str>,
        ref_label: Option<Cow<'a, str>>,
        kind: LinkKind,
    },
    Image {
        dest: Cow<'a, str>,
        title: Cow<'a, str>,
        ref_label: Option<Cow<'a, str>>,
        kind: LinkKind,
    },
    Autolink {
        url: Cow<'a, str>,
        kind: AutolinkKind,
    },
    InlineHtml(Cow<'a, str>),
    FootnoteReference(Cow<'a, str>),
    SoftBreak,
    HardBreak,
    TaskListMarker(bool),

    /// Forward-compatibility fallback. Pulldown-cmark may emit tags
    /// we don't recognise (math when enabled, definition lists,
    /// super/subscript, wiki links, metadata blocks). Rather than
    /// panicking, the builder records an `Unknown` node with the raw
    /// range; the formatter falls back to byte-verbatim emission.
    Unknown {
        tag: &'static str,
    },
}

/// Classification of a [`Link`](NodeKind::Link) or
/// [`Image`](NodeKind::Image).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// `[text](url)`
    Inline,
    /// `[text][label]`
    ReferenceFull,
    /// `[label][]`
    ReferenceCollapsed,
    /// `[label]`
    ReferenceShortcut,
}

/// Classification of an [`Autolink`](NodeKind::Autolink).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AutolinkKind {
    /// `<https://example.com>`
    Uri,
    /// `<user@example.com>`
    Email,
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
pub struct Tree<'a> {
    source: &'a str,
    arena: Vec<Node<'a>>,
    child_ids: Vec<NodeId>,
    parents: Vec<Option<NodeId>>,
}

impl<'a> Tree<'a> {
    /// The source string the tree was parsed from.
    #[must_use]
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// The Document root. Always present.
    #[must_use]
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Look up a node by id. Returns `None` for ids that did not come
    /// from this tree.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node<'a>> {
        self.arena.get(id.idx())
    }

    /// Source bytes covered by `id`. Empty string for ids that did not
    /// come from this tree; otherwise always a valid slice.
    #[must_use]
    pub fn raw_text(&self, id: NodeId) -> &'a str {
        self.node(id)
            .and_then(|n| self.source.get(n.raw_range.clone()))
            .unwrap_or("")
    }

    /// Direct children of `id` in source order.
    pub fn children(&self, id: NodeId) -> Children<'_, 'a> {
        let range = self.node(id).map_or(0..0, |n| n.children.clone());
        Children { tree: self, range }
    }

    /// Parent of `id`, or `None` for the root and for unknown ids.
    #[must_use]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parents.get(id.idx()).copied().flatten()
    }

    /// Every descendant of `id` in pre-order (excluding `id` itself).
    pub fn descendants(&self, id: NodeId) -> Descendants<'_, 'a> {
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
pub struct Children<'t, 'a> {
    tree: &'t Tree<'a>,
    range: Range<u32>,
}

impl Iterator for Children<'_, '_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.range.next()?;
        self.tree.child_ids.get(i as usize).copied()
    }
}

/// Iterator over a node's descendants in pre-order. Returned by
/// [`Tree::descendants`].
pub struct Descendants<'t, 'a> {
    tree: &'t Tree<'a>,
    next: u32,
    end: u32,
}

impl Iterator for Descendants<'_, '_> {
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
pub(crate) struct TreeBuilder<'a> {
    source: &'a str,
    arena: Vec<Node<'a>>,
    child_ids: Vec<NodeId>,
    parents: Vec<Option<NodeId>>,
    /// Scratch buffer; the tail beyond `open.last().pending_start` is
    /// the current open frame's accumulated children.
    pending: Vec<NodeId>,
    open: Vec<OpenFrame>,
}

#[derive(Debug)]
struct OpenFrame {
    arena_id: NodeId,
    pending_start: u32,
    raw_start: usize,
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
            }],
        }
    }

    #[allow(clippy::wildcard_enum_match_arm)]
    pub(crate) fn handle(&mut self, event: &Event<'a>, range: Range<usize>) {
        match event {
            Event::Start(tag) => {
                let kind = self.kind_for_start(tag, &range);
                self.open_container(kind, range);
            }
            Event::End(_) => {
                self.close_container(range);
            }
            Event::Text(cow) => {
                let raw_range = self.extend_for_backslash(range);
                self.push_leaf(NodeKind::Text(cow_to_cow(cow)), raw_range);
            }
            Event::Code(cow) => {
                self.push_leaf(NodeKind::Code(cow_to_cow(cow)), range);
            }
            Event::Html(cow) | Event::InlineHtml(cow) => {
                // Distinguish at the kind: HTML blocks are not wrapped
                // by a container event, so we just store them as a
                // leaf carrying the raw text.
                let is_block = matches!(event, Event::Html(_));
                let kind = if is_block {
                    // An HtmlBlock event is itself a leaf in pulldown's
                    // stream (multiple Event::Html lines may appear
                    // inside a Tag::HtmlBlock, which is a container).
                    NodeKind::Text(cow_to_cow(cow))
                } else {
                    NodeKind::InlineHtml(cow_to_cow(cow))
                };
                self.push_leaf(kind, range);
            }
            Event::FootnoteReference(label) => {
                self.push_leaf(NodeKind::FootnoteReference(cow_to_cow(label)), range);
            }
            Event::SoftBreak => self.push_leaf(NodeKind::SoftBreak, range),
            Event::HardBreak => self.push_leaf(NodeKind::HardBreak, range),
            Event::Rule => self.push_leaf(NodeKind::ThematicBreak, range),
            Event::TaskListMarker(checked) => {
                // Also annotate the enclosing Item so consumers can
                // read `Item.task` without walking children.
                if let Some(frame) = self.open.last()
                    && let Some(node) = self.arena.get_mut(frame.arena_id.idx())
                    && let NodeKind::Item { ref mut task } = node.kind
                {
                    *task = Some(*checked);
                }
                self.push_leaf(NodeKind::TaskListMarker(*checked), range);
            }
            // Math is not enabled in Options; if it ever appears,
            // record it as Unknown leaves so we don't panic.
            Event::InlineMath(cow) | Event::DisplayMath(cow) => {
                self.push_leaf(NodeKind::Text(cow_to_cow(cow)), range);
            }
        }
    }

    /// Synthesise `LinkReferenceDefinition` nodes from the flat IR's
    /// link-defs vector (pulldown does not emit events for them) and
    /// seal the Document root.
    pub(crate) fn finalize(mut self, link_defs: &[LinkDef<'a>]) -> Tree<'a> {
        // The Document frame is still open. Close it. `new` always
        // pushed exactly one frame, so this pop must succeed; if it
        // ever doesn't, fall through with no Document children.
        let doc_pending_start = self.open.pop().map_or(0u32, |f| f.pending_start);
        let mut doc_children: Vec<NodeId> =
            self.pending.drain(doc_pending_start as usize..).collect();

        // Synthesise one LinkReferenceDefinition per link_def and
        // append to doc_children, then sort by raw_range.start so
        // they appear in source order alongside the rest.
        for def in link_defs {
            let id = NodeId(u32::try_from(self.arena.len()).unwrap_or(u32::MAX));
            self.arena.push(Node {
                kind: NodeKind::LinkReferenceDefinition {
                    label: Cow::Borrowed(def.label),
                    dest: Cow::Borrowed(def.dest),
                    title: def.title.map(Cow::Borrowed),
                },
                raw_range: def.raw_range.clone(),
                children: 0..0,
                subtree_end: id.0.saturating_add(1),
            });
            self.parents.push(Some(NodeId(0)));
            doc_children.push(id);
        }
        doc_children.sort_by_key(|id| {
            self.arena
                .get(id.idx())
                .map_or(usize::MAX, |n| n.raw_range.start)
        });

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
            source: self.source,
            arena: self.arena,
            child_ids: self.child_ids,
            parents: self.parents,
        }
    }

    fn alloc_node(&mut self, kind: NodeKind<'a>, raw_range: Range<usize>) -> NodeId {
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

    fn open_container(&mut self, kind: NodeKind<'a>, range: Range<usize>) {
        let raw_start = range.start;
        let id = self.alloc_node(kind, range);
        let pending_start = u32::try_from(self.pending.len()).unwrap_or(u32::MAX);
        self.open.push(OpenFrame {
            arena_id: id,
            pending_start,
            raw_start,
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

        // Compute list tightness now that we know our children.
        let tight = if drained_len == 0 {
            true
        } else {
            let kind_is_list = matches!(
                self.arena.get(frame.arena_id.idx()).map(|n| &n.kind),
                Some(NodeKind::List { .. })
            );
            if kind_is_list {
                self.compute_list_tight(children_start..children_end)
            } else {
                true
            }
        };

        if let Some(node) = self.arena.get_mut(frame.arena_id.idx()) {
            node.children = children_start..children_end;
            node.subtree_end = subtree_end;
            node.raw_range = frame.raw_start..range.end;
            if let NodeKind::List {
                tight: ref mut t, ..
            } = node.kind
            {
                *t = tight;
            }
        }
    }

    /// A list is loose iff any direct `Item` child has a direct
    /// `Paragraph` child. Pulldown elides the Paragraph wrapper inside
    /// tight items, so this is a purely structural test.
    fn compute_list_tight(&self, item_range: Range<u32>) -> bool {
        for i in item_range {
            let Some(&item_id) = self.child_ids.get(i as usize) else {
                continue;
            };
            let Some(item_node) = self.arena.get(item_id.idx()) else {
                continue;
            };
            if !matches!(item_node.kind, NodeKind::Item { .. }) {
                continue;
            }
            for j in item_node.children.clone() {
                let Some(&child_id) = self.child_ids.get(j as usize) else {
                    continue;
                };
                if matches!(
                    self.arena.get(child_id.idx()).map(|n| &n.kind),
                    Some(NodeKind::Paragraph)
                ) {
                    return false;
                }
            }
        }
        true
    }

    /// Handle the tail portion of an event whose source range
    fn push_leaf(&mut self, kind: NodeKind<'a>, range: Range<usize>) {
        let _ = self.alloc_node(kind, range);
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

    fn kind_for_start(&self, tag: &Tag<'a>, range: &Range<usize>) -> NodeKind<'a> {
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
                    CodeBlockKind::Fenced(s) => (true, cow_to_cow(s)),
                    CodeBlockKind::Indented => (false, Cow::Borrowed("")),
                };
                NodeKind::CodeBlock { fenced, info }
            }
            Tag::HtmlBlock => NodeKind::HtmlBlock,
            Tag::List(start) => NodeKind::List {
                ordered: start.is_some(),
                start: start.unwrap_or(0),
                tight: true,
                marker_byte: first_non_whitespace_byte(self.source, range.start).unwrap_or(0),
            },
            Tag::Item => NodeKind::Item { task: None },
            Tag::FootnoteDefinition(label) => NodeKind::FootnoteDefinition {
                label: cow_to_cow(label),
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

fn link_kind<'a>(
    lt: LinkType,
    dest_url: &CowStr<'a>,
    title: &CowStr<'a>,
    id: &CowStr<'a>,
    is_image: bool,
) -> NodeKind<'a> {
    match lt {
        LinkType::Autolink => NodeKind::Autolink {
            url: cow_to_cow(dest_url),
            kind: AutolinkKind::Uri,
        },
        LinkType::Email => NodeKind::Autolink {
            url: cow_to_cow(dest_url),
            kind: AutolinkKind::Email,
        },
        LinkType::WikiLink { .. } => NodeKind::Unknown { tag: "WikiLink" },
        LinkType::Inline
        | LinkType::Reference
        | LinkType::ReferenceUnknown
        | LinkType::Collapsed
        | LinkType::CollapsedUnknown
        | LinkType::Shortcut
        | LinkType::ShortcutUnknown => {
            let kind = match lt {
                LinkType::Inline => LinkKind::Inline,
                LinkType::Reference | LinkType::ReferenceUnknown => LinkKind::ReferenceFull,
                LinkType::Collapsed | LinkType::CollapsedUnknown => LinkKind::ReferenceCollapsed,
                LinkType::Shortcut | LinkType::ShortcutUnknown => LinkKind::ReferenceShortcut,
                LinkType::Autolink | LinkType::Email | LinkType::WikiLink { .. } => {
                    LinkKind::Inline
                }
            };
            let ref_label = if id.is_empty() {
                None
            } else {
                Some(cow_to_cow(id))
            };
            let dest = cow_to_cow(dest_url);
            let title = cow_to_cow(title);
            if is_image {
                NodeKind::Image {
                    dest,
                    title,
                    ref_label,
                    kind,
                }
            } else {
                NodeKind::Link {
                    dest,
                    title,
                    ref_label,
                    kind,
                }
            }
        }
    }
}

/// Convert a pulldown `CowStr` into a standard `Cow<str>` without
/// copying when the input is borrowed.
fn cow_to_cow<'a>(s: &CowStr<'a>) -> Cow<'a, str> {
    match s {
        CowStr::Borrowed(b) => Cow::Borrowed(b),
        CowStr::Boxed(b) => Cow::Owned(b.to_string()),
        CowStr::Inlined(i) => Cow::Owned(i.to_string()),
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
        let kinds: Vec<&NodeKind<'_>> = tree
            .descendants(tree.root())
            .filter_map(|id| tree.node(id).map(|n| &n.kind))
            .collect();
        assert!(kinds.iter().any(|k| matches!(k, NodeKind::Paragraph)));
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, NodeKind::Text(s) if s.contains("Hello")))
        );
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

    fn find_list_tight(tree: &Tree<'_>) -> Option<bool> {
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
                Some(NodeKind::Link {
                    kind, ref_label, ..
                }) => Some((*kind, ref_label.clone())),
                _ => None,
            })
            .expect("link present");
        assert_eq!(link.0, LinkKind::ReferenceFull);
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
                Some(NodeKind::Link { kind, .. }) => Some(*kind),
                _ => None,
            })
            .expect("link present");
        assert_eq!(kind, LinkKind::ReferenceCollapsed);
    }

    #[test]
    fn shortcut_reference_link() {
        let src = "[foo]\n\n[foo]: https://example.com\n";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        let kind = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Link { kind, .. }) => Some(*kind),
                _ => None,
            })
            .expect("link present");
        assert_eq!(kind, LinkKind::ReferenceShortcut);
    }

    #[test]
    fn link_reference_definitions_appear_as_doc_children() {
        let src = "[a]: https://a.example\n[b]: https://b.example\n\nText.\n";
        let ir = Ir::parse(src);
        let tree = &ir.tree;
        let defs: Vec<String> = tree
            .children(tree.root())
            .filter_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::LinkReferenceDefinition { label, .. }) => Some(label.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(defs, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn autolink_kind() {
        let ir = Ir::parse("<https://example.com>\n");
        let tree = &ir.tree;
        let url = tree
            .descendants(tree.root())
            .find_map(|id| match tree.node(id).map(|n| &n.kind) {
                Some(NodeKind::Autolink {
                    url,
                    kind: AutolinkKind::Uri,
                }) => Some(url.to_string()),
                _ => None,
            })
            .expect("autolink present");
        assert!(url.starts_with("https://"));
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
                Some(NodeKind::CodeBlock { fenced: true, info }) => Some(info.to_string()),
                _ => None,
            })
            .expect("fenced code block");
        assert_eq!(info, "rust");
    }
}
