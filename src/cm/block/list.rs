//! Bullet and ordered lists (CM §5.2–§5.3) plus GFM task items (§5.3).
//!
//! [`Tightness`] is a *derived* fact of a [`ListBlock`]'s items: CM §5.3
//! defines a list as tight iff no direct item contains a direct
//! [`Paragraph`](crate::tree::NodeKind::Paragraph) child. We materialise
//! that derivation as [`Tightness::from_items`] and call it inside
//! [`ListBlock::try_new`]; there is no public setter for the field.
//! Reformatting an item that flips its paragraph-child shape changes
//! the reconstructed `ListBlock`'s tightness automatically — the
//! Phase-4 idempotence-flip bug becomes impossible by construction.
//!
//! Task items piggy-back: the checkbox marker is a structural
//! property of the item (GFM §5.3 places it as the first inline of
//! the first paragraph), so we materialise it as a `bool` on
//! [`TaskItem`]. The marker cannot drift away from that position
//! because nothing else carries it.
//!
//! Items themselves live in the surrounding [`crate::tree::Tree`]
//! arena as `NodeKind::Item` nodes; this typed value carries only the
//! per-item *facts* the legacy `NodeKind::List { tight, marker_byte }`
//! cannot encode as type-level invariants. Each [`ListItemKind`]
//! records the arena id of its source `Node` so emitters can rejoin
//! to the inline body.

use std::borrow::Cow;

use crate::tree::NodeId;

/// CM §5.3 tightness. *Derived* from items at [`ListBlock::try_new`];
/// no constructor accepts it as input.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tightness {
    /// No item contains a direct `Paragraph` child.
    Tight,
    /// At least one item contains a direct `Paragraph` child.
    Loose,
}

impl Tightness {
    /// Pure derivation: tight iff every item's
    /// `has_direct_paragraph` flag is false.
    pub(crate) fn from_items(items: &[ListItemKind]) -> Self {
        if items.iter().any(ListItemKind::body_has_direct_paragraph) {
            Self::Loose
        } else {
            Self::Tight
        }
    }
}

/// Punctuation after an ordered marker. CM §5.2 allows `.` (`1.`) or
/// `)` (`1)`); the choice survives in the IR so an emitter that wants
/// to preserve source style can.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum OrderedDelim {
    Period,
    Paren,
}

impl OrderedDelim {
    pub(crate) fn as_char(self) -> char {
        match self {
            Self::Period => '.',
            Self::Paren => ')',
        }
    }
}

/// Bullet/ordered marker of a list. Carries the *parsed* shape; any
/// normalisation (e.g., "always emit `-` for bullets") is a formatter
/// decision applied at render time.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ListMarker {
    Dash,
    Asterisk,
    Plus,
    Ordered { start: u32, delim: OrderedDelim },
}

impl ListMarker {
    /// Lift the legacy `(ordered, start, marker_byte)` triple into a
    /// `ListMarker`. Returns `None` for byte/ordered combinations that
    /// could not have come from a CM list (defensive — pulldown should
    /// never produce them).
    pub(crate) fn from_legacy(
        ordered: bool,
        start: u64,
        marker_byte: u8,
        source: &str,
        raw_range: core::ops::Range<usize>,
    ) -> Option<Self> {
        if ordered {
            let start = u32::try_from(start).ok()?;
            let delim = scan_ordered_delim(source, raw_range).unwrap_or(OrderedDelim::Period);
            Some(Self::Ordered { start, delim })
        } else {
            match marker_byte {
                b'-' => Some(Self::Dash),
                b'*' => Some(Self::Asterisk),
                b'+' => Some(Self::Plus),
                _ => None,
            }
        }
    }
}

/// Walk the first item's leading bytes for `.` or `)` after the ordered
/// digits. Returns `None` if neither is present in the range.
fn scan_ordered_delim(source: &str, raw_range: core::ops::Range<usize>) -> Option<OrderedDelim> {
    let bytes = source.as_bytes().get(raw_range)?;
    // Skip leading whitespace, then digits, then look at the next byte.
    let mut i = 0;
    while bytes.get(i).is_some_and(|b| matches!(*b, b' ' | b'\t')) {
        i = i.saturating_add(1);
    }
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i = i.saturating_add(1);
    }
    match bytes.get(i).copied() {
        Some(b'.') => Some(OrderedDelim::Period),
        Some(b')') => Some(OrderedDelim::Paren),
        _ => None,
    }
}

/// A plain list item.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListItem {
    item_id: NodeId,
    indent: u8,
    has_direct_paragraph: bool,
}

impl ListItem {
    pub(crate) fn new(item_id: NodeId, indent: u8, has_direct_paragraph: bool) -> Self {
        Self {
            item_id,
            indent,
            has_direct_paragraph,
        }
    }

    pub(crate) fn item_id(self) -> NodeId {
        self.item_id
    }

    pub(crate) fn indent(self) -> u8 {
        self.indent
    }

    pub(crate) fn has_direct_paragraph(self) -> bool {
        self.has_direct_paragraph
    }
}

/// A GFM §5.3 task list item. `checked` materialises the bracketed
/// marker (`[x]` vs `[ ]`) as a structural fact; `body_empty` is `true`
/// when the item contains only the task marker (no following inline
/// content). The emitter renders `[ ] ` / `[x] ` with no trailing
/// content in that case — it never invents a placeholder paragraph.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct TaskItem {
    item_id: NodeId,
    indent: u8,
    has_direct_paragraph: bool,
    checked: bool,
    body_empty: bool,
}

impl TaskItem {
    pub(crate) fn new(
        item_id: NodeId,
        indent: u8,
        has_direct_paragraph: bool,
        checked: bool,
        body_empty: bool,
    ) -> Self {
        Self {
            item_id,
            indent,
            has_direct_paragraph,
            checked,
            body_empty,
        }
    }

    pub(crate) fn item_id(self) -> NodeId {
        self.item_id
    }

    pub(crate) fn indent(self) -> u8 {
        self.indent
    }

    pub(crate) fn has_direct_paragraph(self) -> bool {
        self.has_direct_paragraph
    }

    pub(crate) fn checked(self) -> bool {
        self.checked
    }

    pub(crate) fn body_empty(self) -> bool {
        self.body_empty
    }
}

/// One item position within a [`ListBlock`]. `Plain` for ordinary
/// items, `Task` for the GFM checkbox extension.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ListItemKind {
    Plain(ListItem),
    Task(TaskItem),
}

impl ListItemKind {
    pub(crate) fn item_id(self) -> NodeId {
        match self {
            Self::Plain(p) => p.item_id(),
            Self::Task(t) => t.item_id(),
        }
    }

    pub(crate) fn body_has_direct_paragraph(&self) -> bool {
        match self {
            Self::Plain(p) => p.has_direct_paragraph(),
            Self::Task(t) => t.has_direct_paragraph(),
        }
    }
}

/// A list whose existence guarantees CM §5.3 tightness is *derived*
/// from its items — there is no constructor path that lets the
/// caller name a tightness that disagrees with the items.
#[derive(Clone, Debug)]
pub(crate) struct ListBlock {
    marker: ListMarker,
    tightness: Tightness,
    items: Vec<ListItemKind>,
}

impl ListBlock {
    /// Construct from a parsed marker and the per-item facts.
    /// Returns [`ListBlockError::Empty`] if `items` is empty (a list
    /// with no items is a degenerate pulldown shape — leave the
    /// typed view absent and fall back to legacy emission).
    #[tracing::instrument(level = "trace", skip_all, fields(marker = ?marker, items_len = items.len()))]
    pub(crate) fn try_new(
        marker: ListMarker,
        items: Vec<ListItemKind>,
    ) -> Result<Self, ListBlockError> {
        if items.is_empty() {
            return Err(ListBlockError::Empty);
        }
        let tightness = Tightness::from_items(&items);
        tracing::event!(tracing::Level::TRACE, ?tightness, "derived");
        Ok(Self {
            marker,
            tightness,
            items,
        })
    }

    pub(crate) fn marker(&self) -> ListMarker {
        self.marker
    }

    pub(crate) fn tightness(&self) -> Tightness {
        self.tightness
    }

    /// `true` iff this list uses an unordered marker (`-`, `*`, `+`).
    /// Ordered lists distinguish themselves by `start`; bullet adjacency
    /// resolution applies only to the unordered case.
    pub(crate) fn is_unordered(&self) -> bool {
        matches!(
            self.marker,
            ListMarker::Dash | ListMarker::Asterisk | ListMarker::Plus
        )
    }

    /// Source bullet byte (`-`, `*`, or `+`) for unordered lists.
    /// Panics on ordered lists — gate with [`Self::is_unordered`].
    fn source_bullet_byte(&self) -> u8 {
        match self.marker {
            ListMarker::Dash => b'-',
            ListMarker::Asterisk => b'*',
            ListMarker::Plus => b'+',
            ListMarker::Ordered { .. } => {
                debug_assert!(false, "source_bullet_byte on ordered list");
                b'-'
            }
        }
    }

    /// Pick a bullet byte for emission that does not collide with the
    /// immediately preceding adjacent unordered list's emitted bullet.
    ///
    /// CM §5.2: pulldown distinguishes adjacent lists by their marker
    /// character. Per-list normalisation (`ListMarkerStyle::Dash` &c.)
    /// can otherwise unify two source-distinct lists into one — the
    /// fuzz-found `+\n-` case.
    ///
    /// Strategy: prefer the configured style; if that would collide
    /// with `avoid`, fall back to the source bullet (guaranteed to
    /// differ from any adjacent list's source bullet — otherwise
    /// pulldown would have parsed them as a single list); if even the
    /// source bullet collides, pick any byte from `{-, *, +}` that
    /// avoids the collision.
    pub(crate) fn resolve_unordered_bullet(
        &self,
        opts: &crate::config::FmtOptions,
        avoid: Option<u8>,
    ) -> u8 {
        debug_assert!(self.is_unordered());
        let source_byte = self.source_bullet_byte();
        let candidate = opts.resolve_list_marker(source_byte);
        if avoid != Some(candidate) {
            return candidate;
        }
        if Some(source_byte) != avoid {
            return source_byte;
        }
        // All three are valid CM bullets; pick any byte that avoids.
        [b'-', b'*', b'+']
            .into_iter()
            .find(|&b| Some(b) != avoid)
            .unwrap_or(b'-')
    }

    pub(crate) fn items(&self) -> &[ListItemKind] {
        &self.items
    }

    /// Lookup helper: typed view for the item whose arena id is
    /// `node_id`, if it belongs to this list.
    pub(crate) fn item_for(&self, node_id: NodeId) -> Option<&ListItemKind> {
        self.items.iter().find(|k| k.item_id() == node_id)
    }

    /// Emit every item in source order, marker-prefixed, with
    /// continuation lines indented to the marker width. Loose lists
    /// insert a blank line between items. Always terminates with one
    /// hard newline so the surrounding block-sequence separator
    /// produces a blank line between the list and whatever follows.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'a>(
        &self,
        ctx: &crate::format::pretty::PrettyCtx<'a>,
        id: crate::tree::NodeId,
    ) -> crate::format::doc::Doc<'a> {
        self.pretty_with_bullet(ctx, id, None)
    }

    /// Same as [`Self::pretty`] but the caller (the block-sequence
    /// loop) supplies an already-resolved unordered bullet so adjacent
    /// lists can avoid collision. `None` ⇒ no adjacency constraint
    /// (fresh sequence or ordered list).
    pub(crate) fn pretty_with_bullet<'a>(
        &self,
        ctx: &crate::format::pretty::PrettyCtx<'a>,
        _id: crate::tree::NodeId,
        unordered_bullet: Option<u8>,
    ) -> crate::format::doc::Doc<'a> {
        use crate::format::doc::{concat, hard_line};
        let tight = matches!(self.tightness, Tightness::Tight);
        let mut parts: Vec<crate::format::doc::Doc<'a>> =
            Vec::with_capacity(self.items.len().saturating_mul(2));
        // Each rendered item's body already ends with a `HardLine`
        // (the block-helper contract). For tight lists that hard
        // line is the only between-items separator we want; for
        // loose lists we add one more to make a blank line.
        for (idx, item_kind) in self.items.iter().enumerate() {
            if idx > 0 && !tight {
                parts.push(hard_line());
            }
            let marker = self.marker_for_index(ctx, idx, item_kind, unordered_bullet);
            parts.push(render_item(ctx, item_kind, &marker));
        }
        concat(parts)
    }

    fn marker_for_index(
        &self,
        ctx: &crate::format::pretty::PrettyCtx<'_>,
        idx: usize,
        item_kind: &ListItemKind,
        unordered_bullet: Option<u8>,
    ) -> String {
        use crate::config::OrderedListStyle;
        match self.marker {
            ListMarker::Ordered { start, delim } => {
                let n = match ctx.opts.ordered_list() {
                    OrderedListStyle::Consistent => u64::from(start).saturating_add(idx as u64),
                    OrderedListStyle::Preserve => {
                        source_ordered_marker_number(ctx, item_kind.item_id())
                            .unwrap_or_else(|| u64::from(start).saturating_add(idx as u64))
                    }
                };
                let punct = source_ordered_punct(ctx, item_kind.item_id())
                    .unwrap_or_else(|| delim.as_char());
                format!("{n}{punct} ")
            }
            ListMarker::Dash | ListMarker::Asterisk | ListMarker::Plus => {
                let b = unordered_bullet
                    .unwrap_or_else(|| ctx.opts.resolve_list_marker(self.source_bullet_byte()));
                format!("{} ", char::from(b))
            }
        }
    }
}

fn source_ordered_marker_number(
    ctx: &crate::format::pretty::PrettyCtx<'_>,
    item_id: NodeId,
) -> Option<u64> {
    let raw = ctx.tree.raw_text(item_id);
    let trimmed = raw.trim_start();
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn source_ordered_punct(
    ctx: &crate::format::pretty::PrettyCtx<'_>,
    item_id: NodeId,
) -> Option<char> {
    let raw = ctx.tree.raw_text(item_id);
    let trimmed = raw.trim_start();
    trimmed
        .chars()
        .find(|c| !c.is_ascii_digit())
        .filter(|c| *c == '.' || *c == ')')
}

fn render_item<'a>(
    ctx: &crate::format::pretty::PrettyCtx<'a>,
    item_kind: &ListItemKind,
    marker: &str,
) -> crate::format::doc::Doc<'a> {
    use crate::format::doc::{LinePrefix, concat, prefix_lines, text, unbreakable};

    let id = item_kind.item_id();
    let task_prefix = match item_kind {
        ListItemKind::Task(t) => Some(if t.checked() { "[x] " } else { "[ ] " }),
        ListItemKind::Plain(_) => None,
    };

    let body = render_item_body(ctx, id);
    let marker_with_task: String = match task_prefix {
        Some(t) => format!("{marker}{t}"),
        None => marker.to_owned(),
    };
    let indent_width = marker_with_task.chars().count();
    let indent: Cow<'static, str> = indent_cow(indent_width);
    let prefixed = prefix_lines(
        LinePrefix {
            content: indent,
            blank: "".into(),
        },
        body,
    );
    // `unbreakable(text(marker))` keeps the marker's trailing space
    // off the wrap pass's whitespace-strip path (see the symmetric
    // note in `cm::block::quote::BlockQuote::pretty`). The Prefix'd
    // body itself is left open to wrap — its inner content needs
    // continuation lines to break inside the reduced budget.
    concat([unbreakable(text(marker_with_task)), prefixed])
}

/// Lookup `' '` × `n` as a static slice when `n ≤ 32`; allocate
/// otherwise. List-item indent widths rarely exceed a single-digit
/// ordered marker + `[x] `, so the static path is the common case.
fn indent_cow(n: usize) -> Cow<'static, str> {
    const SPACES: &str = "                                "; // 32 spaces
    if n <= SPACES.len() {
        Cow::Borrowed(&SPACES[..n])
    } else {
        Cow::Owned(" ".repeat(n))
    }
}

/// Render an `Item`'s children: groups runs of inline children into
/// virtual paragraphs and recurses into block children normally. When
/// the parent list is loose, item-internal blocks are separated by a
/// blank line.
fn render_item_body<'a>(
    ctx: &crate::format::pretty::PrettyCtx<'a>,
    id: NodeId,
) -> crate::format::doc::Doc<'a> {
    use crate::cm::block::paragraph::ParagraphBody;
    use crate::format::doc::{concat, hard_line};
    use crate::tree::NodeKind;

    let parent_loose = ctx
        .tree
        .parent(id)
        .and_then(|p| ctx.tree.node(p))
        .is_some_and(|n| matches!(n.kind, NodeKind::List { tight: false, .. }));
    let children: Vec<NodeId> = ctx.tree.children(id).collect();
    let mut parts: Vec<crate::format::doc::Doc<'a>> = Vec::new();
    let mut inline_run: Vec<NodeId> = Vec::new();
    let mut emitted = 0usize;

    let flush_inline = |run: &mut Vec<NodeId>,
                        parts: &mut Vec<crate::format::doc::Doc<'a>>,
                        emitted: &mut usize| {
        if run.is_empty() {
            return;
        }
        if *emitted > 0 && parent_loose {
            parts.push(hard_line());
        }
        let inline = crate::format::inline::pretty_inline_children_for_ids(ctx, run);
        let body = ParagraphBody::from_inline(inline).into_doc();
        parts.push(concat([body, hard_line()]));
        *emitted = emitted.saturating_add(1);
        run.clear();
    };

    for cid in children {
        let kind = ctx.tree.node(cid).map(|n| &n.kind);
        if is_block_kind(kind) {
            flush_inline(&mut inline_run, &mut parts, &mut emitted);
            if emitted > 0 && parent_loose {
                parts.push(hard_line());
            }
            parts.push(crate::format::block::pretty_block(ctx, cid));
            emitted = emitted.saturating_add(1);
        } else {
            inline_run.push(cid);
        }
    }
    flush_inline(&mut inline_run, &mut parts, &mut emitted);
    // Empty list items (`*\n*\n*`) have no children, so `parts` would
    // be empty and the item would render as just the marker with no
    // trailing newline. Adjacent items then concatenate on one line,
    // turning into a thematic break (`- - -`) on re-parse. Emit a
    // single hard-line so each item owns its source line.
    if emitted == 0 {
        parts.push(hard_line());
    }
    concat(parts)
}

fn is_block_kind(kind: Option<&crate::tree::NodeKind<'_>>) -> bool {
    use crate::tree::NodeKind;
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
        )
    )
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ListBlockError {
    /// A list with no items. Pulldown can emit this for degenerate
    /// input; the IR builder skips typed construction and falls back
    /// to legacy emission.
    Empty,
    /// Ordered list whose `start` overflows `u32`. CM caps meaningful
    /// markers at 9 digits.
    OrderedStartTooLarge { start: u64 },
}

/// Count leading ASCII space/tab bytes on the first non-blank line of
/// `raw_range`. Used by the IR builder to derive a per-item indent
/// without round-tripping through the pulldown-cmark event stream.
pub(crate) fn item_indent(source: &str, raw_range: core::ops::Range<usize>) -> u8 {
    let bytes = source.as_bytes().get(raw_range).unwrap_or(&[]);
    let mut i = 0;
    // Skip fully-blank leading lines.
    loop {
        let line_start = i;
        while bytes.get(i).is_some_and(|b| *b != b'\n') {
            i = i.saturating_add(1);
        }
        let line_bytes = bytes.get(line_start..i).unwrap_or(&[]);
        if line_bytes
            .iter()
            .any(|b| !matches!(*b, b' ' | b'\t' | b'\r'))
        {
            // Non-blank line — count its indent.
            let count = line_bytes
                .iter()
                .take_while(|b| matches!(**b, b' ' | b'\t'))
                .count();
            return u8::try_from(count).unwrap_or(u8::MAX);
        }
        if i >= bytes.len() {
            return 0;
        }
        // Step past the newline and continue.
        i = i.saturating_add(1);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn nid(i: u32) -> NodeId {
        // NodeId is a public newtype with private field, but the
        // crate-internal constructor is not exposed. We synthesize
        // ids by parsing a tiny document whose Tree assigns them
        // deterministically.
        use crate::ir::Ir;
        let src = "- a\n- b\n- c\n- d\n";
        let ir = Ir::parse(src);
        ir.tree
            .descendants(ir.tree.root())
            .nth(i as usize)
            .expect("ir has nodes")
    }

    #[test]
    fn tightness_all_tight_when_no_paragraph_children() {
        let items = vec![
            ListItemKind::Plain(ListItem::new(nid(0), 0, false)),
            ListItemKind::Plain(ListItem::new(nid(1), 0, false)),
        ];
        assert_eq!(Tightness::from_items(&items), Tightness::Tight);
    }

    #[test]
    fn tightness_loose_when_any_item_has_paragraph() {
        let items = vec![
            ListItemKind::Plain(ListItem::new(nid(0), 0, false)),
            ListItemKind::Plain(ListItem::new(nid(1), 0, true)),
        ];
        assert_eq!(Tightness::from_items(&items), Tightness::Loose);
    }

    #[test]
    fn tightness_loose_when_task_item_has_paragraph() {
        let items = vec![ListItemKind::Task(TaskItem::new(
            nid(0),
            0,
            true,
            false,
            false,
        ))];
        assert_eq!(Tightness::from_items(&items), Tightness::Loose);
    }

    #[test]
    fn try_new_rejects_empty() {
        assert_eq!(
            ListBlock::try_new(ListMarker::Dash, vec![]).err(),
            Some(ListBlockError::Empty),
        );
    }

    #[test]
    fn try_new_derives_tightness() {
        let lb = ListBlock::try_new(
            ListMarker::Dash,
            vec![ListItemKind::Plain(ListItem::new(nid(0), 0, false))],
        )
        .expect("non-empty");
        assert_eq!(lb.tightness(), Tightness::Tight);

        let lb = ListBlock::try_new(
            ListMarker::Dash,
            vec![ListItemKind::Plain(ListItem::new(nid(0), 0, true))],
        )
        .expect("non-empty");
        assert_eq!(lb.tightness(), Tightness::Loose);
    }

    #[test]
    fn marker_from_legacy_bullet() {
        assert_eq!(
            ListMarker::from_legacy(false, 0, b'-', "- a\n", 0..4),
            Some(ListMarker::Dash),
        );
        assert_eq!(
            ListMarker::from_legacy(false, 0, b'*', "* a\n", 0..4),
            Some(ListMarker::Asterisk),
        );
        assert_eq!(
            ListMarker::from_legacy(false, 0, b'+', "+ a\n", 0..4),
            Some(ListMarker::Plus),
        );
        assert_eq!(ListMarker::from_legacy(false, 0, b'!', "! a\n", 0..4), None,);
    }

    #[test]
    fn marker_from_legacy_ordered_period_and_paren() {
        assert_eq!(
            ListMarker::from_legacy(true, 1, b'1', "1. a\n", 0..5),
            Some(ListMarker::Ordered {
                start: 1,
                delim: OrderedDelim::Period,
            }),
        );
        assert_eq!(
            ListMarker::from_legacy(true, 3, b'3', "3) a\n", 0..5),
            Some(ListMarker::Ordered {
                start: 3,
                delim: OrderedDelim::Paren,
            }),
        );
    }

    #[test]
    fn marker_from_legacy_ordered_start_overflow_rejected() {
        // 2^32 does not fit in u32; the constructor returns None.
        let huge: u64 = u64::from(u32::MAX) + 1;
        assert_eq!(
            ListMarker::from_legacy(true, huge, b'1', "1. a\n", 0..5),
            None,
        );
    }

    #[test]
    fn item_indent_counts_leading_spaces() {
        assert_eq!(item_indent("    - a\n", 0..8), 4);
        assert_eq!(item_indent("- a\n", 0..4), 0);
        assert_eq!(item_indent("\t- a\n", 0..5), 1);
        // Blank leading line is skipped.
        assert_eq!(item_indent("\n  - a\n", 0..7), 2);
    }

    #[test]
    fn item_for_finds_by_id() {
        let id_a = nid(0);
        let id_b = nid(1);
        let lb = ListBlock::try_new(
            ListMarker::Dash,
            vec![
                ListItemKind::Plain(ListItem::new(id_a, 0, false)),
                ListItemKind::Plain(ListItem::new(id_b, 0, false)),
            ],
        )
        .expect("non-empty");
        assert!(lb.item_for(id_a).is_some());
        assert!(lb.item_for(id_b).is_some());
    }
}
