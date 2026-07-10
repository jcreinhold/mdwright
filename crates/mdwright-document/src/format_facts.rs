//! Source-coordinate facts used by formatter rewrite passes.
//!
//! The formatter needs byte ranges for recognised Markdown constructs,
//! but it should not know how `pulldown-cmark` exposes those ranges.
//! This module converts parser events into small domain records.

#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "document fact queries filter pulldown events and intentionally ignore unrelated variants"
)]

use std::ops::Range;

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::gfm::AutolinkFact;
use crate::heading::find_attr_trailer_range;
use crate::ir::{CodeBlock, HtmlBlock};
use crate::refs::NormalisedLabel;
use crate::tree::{NodeKind, TableAlign, Tree};
use crate::{Document, HeadingAttrs};
use mdwright_math::MathRegion;

/// Structural owner kinds with source ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralKind {
    Paragraph,
    Heading,
    BlockQuote,
    List,
    ListItem,
    DefinitionList,
    DefinitionDescription,
    FootnoteDefinition,
    ThematicBreak,
    Table,
}

/// A recognised block/container range.
#[derive(Clone, Debug)]
pub struct StructuralSpan {
    kind: StructuralKind,
    raw_range: Range<usize>,
}

impl StructuralSpan {
    #[must_use]
    pub fn kind(&self) -> StructuralKind {
        self.kind
    }

    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }
}

/// Inline delimiter kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineDelimiterKind {
    Emphasis,
    Strong,
}

/// Delimiter byte slots for one inline span.
#[derive(Clone, Debug)]
pub struct InlineDelimiterSlot {
    pair: usize,
    kind: InlineDelimiterKind,
    open_lo: usize,
    open_hi: usize,
    close_lo: usize,
    close_hi: usize,
}

impl InlineDelimiterSlot {
    #[must_use]
    pub fn pair(&self) -> usize {
        self.pair
    }

    #[must_use]
    pub fn kind(&self) -> InlineDelimiterKind {
        self.kind
    }

    #[must_use]
    pub fn open_range(&self) -> Range<usize> {
        self.open_lo..self.open_hi
    }

    #[must_use]
    pub fn close_range(&self) -> Range<usize> {
        self.close_lo..self.close_hi
    }
}

/// One unordered list item marker.
#[derive(Clone, Debug)]
pub struct UnorderedListMarkerSite {
    marker: usize,
}

impl UnorderedListMarkerSite {
    #[must_use]
    pub fn marker_range(&self) -> Range<usize> {
        self.marker..self.marker.saturating_add(1)
    }
}

/// One ordered list item marker.
#[derive(Clone, Debug)]
pub struct OrderedListMarkerSite {
    marker_lo: usize,
    marker_hi: usize,
    start_number: u64,
    ordinal: usize,
}

impl OrderedListMarkerSite {
    #[must_use]
    pub fn marker_range(&self) -> Range<usize> {
        self.marker_lo..self.marker_hi
    }

    #[must_use]
    pub fn start_number(&self) -> u64 {
        self.start_number
    }

    #[must_use]
    pub fn ordinal(&self) -> usize {
        self.ordinal
    }
}

/// An ATX heading attribute trailer.
#[derive(Clone, Debug)]
pub struct HeadingAttrSite {
    attrs: HeadingAttrs,
    trailer: Range<usize>,
}

impl HeadingAttrSite {
    #[must_use]
    pub fn attrs(&self) -> &HeadingAttrs {
        &self.attrs
    }

    #[must_use]
    pub fn trailer(&self) -> Range<usize> {
        self.trailer.clone()
    }
}

/// An inline link or image destination byte slot.
#[derive(Clone, Debug)]
pub struct InlineLinkDestinationSlot {
    range: Range<usize>,
}

impl InlineLinkDestinationSlot {
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

/// A link-reference definition destination byte range.
#[derive(Clone, Debug)]
pub struct ReferenceDefinitionSite {
    raw_range: Range<usize>,
    destination: Range<usize>,
}

impl ReferenceDefinitionSite {
    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }

    #[must_use]
    pub fn destination(&self) -> Range<usize> {
        self.destination.clone()
    }
}

/// One GFM table source range and its source rows.
#[derive(Clone, Debug)]
pub struct TableSite {
    raw_range: Range<usize>,
    alignments: Vec<TableAlign>,
    rows: Vec<TableRowSite>,
}

impl TableSite {
    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }

    #[must_use]
    pub fn alignments(&self) -> &[TableAlign] {
        &self.alignments
    }

    #[must_use]
    pub fn rows(&self) -> &[TableRowSite] {
        &self.rows
    }
}

/// One raw table line.
#[derive(Clone, Debug)]
pub struct TableRowSite {
    raw_range: Range<usize>,
    cells: Vec<TableCellSite>,
}

impl TableRowSite {
    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }

    #[must_use]
    pub fn cells(&self) -> &[TableCellSite] {
        &self.cells
    }
}

/// One table cell's source range inside a raw table line.
#[derive(Clone, Debug)]
pub struct TableCellSite {
    raw_range: Range<usize>,
}

impl TableCellSite {
    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }
}

/// The whitespace between two adjacent top-level blocks.
///
/// The range starts at the first line after the preceding block's last
/// content line and ends at the start of the following block's first
/// line, so the following block's 0–3 space indent is never inside it.
/// Replacing the range with `"\n"` leaves exactly one blank line;
/// replacing an empty range inserts one.
///
/// A site exists only when the gap bytes are whitespace. pulldown emits
/// no events for link reference definitions, so a bare `[a]: /x` line
/// between two siblings lands inside the computed gap and suppresses the
/// site rather than being silently deleted. The same guard covers every
/// other construct the tree does not model.
///
/// Only gaps between direct children of the document root are recorded.
/// Inside a block quote a blank line must carry a `>` prefix, and inside
/// a tight list a blank line loosens the list; both are semantic changes,
/// so nested gaps are excluded rather than special-cased.
#[derive(Clone, Debug)]
pub struct BlockGapSite {
    range: Range<usize>,
    prev_is_heading: bool,
    next_is_heading: bool,
}

impl BlockGapSite {
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// Whether the block before the gap is a heading (ATX or setext).
    #[must_use]
    pub fn prev_is_heading(&self) -> bool {
        self.prev_is_heading
    }

    /// Whether the block after the gap is a heading (ATX or setext).
    #[must_use]
    pub fn next_is_heading(&self) -> bool {
        self.next_is_heading
    }
}

/// A paragraph range with the inline facts needed by the wrap pass.
#[derive(Clone, Debug)]
pub struct WrappableParagraph {
    line_lo: usize,
    line_hi: usize,
    content_lo: usize,
    content_hi: usize,
    owner_kind: StructuralKind,
    first_prefix: String,
    cont_prefix: String,
    list_four_space_cont_prefix: Option<String>,
    atomics: Vec<Range<usize>>,
    hard_breaks: Vec<ParagraphHardBreak>,
}

impl WrappableParagraph {
    #[must_use]
    pub fn line_range(&self) -> Range<usize> {
        self.line_lo..self.line_hi
    }

    #[must_use]
    pub fn content_range(&self) -> Range<usize> {
        self.content_lo..self.content_hi
    }

    #[must_use]
    pub fn owner_kind(&self) -> StructuralKind {
        self.owner_kind
    }

    #[must_use]
    pub fn first_prefix(&self) -> &str {
        &self.first_prefix
    }

    #[must_use]
    pub fn cont_prefix(&self) -> &str {
        &self.cont_prefix
    }

    #[must_use]
    pub fn list_four_space_cont_prefix(&self) -> Option<&str> {
        self.list_four_space_cont_prefix.as_deref()
    }

    #[must_use]
    pub fn atomics(&self) -> &[Range<usize>] {
        &self.atomics
    }

    #[must_use]
    pub fn hard_breaks(&self) -> &[ParagraphHardBreak] {
        &self.hard_breaks
    }
}

#[derive(Clone, Debug)]
pub struct ParagraphHardBreak {
    marker_lo: usize,
    nl: usize,
    marker: &'static str,
}

impl ParagraphHardBreak {
    #[must_use]
    pub fn marker_start(&self) -> usize {
        self.marker_lo
    }

    #[must_use]
    pub fn newline(&self) -> usize {
        self.nl
    }

    #[must_use]
    pub fn marker(&self) -> &'static str {
        self.marker
    }
}

/// Cached source-coordinate facts consumed by formatter rewrite passes.
#[derive(Clone, Debug, Default)]
pub(crate) struct FormatFacts {
    structural_spans: Vec<StructuralSpan>,
    emphasis_delimiter_slots: Vec<InlineDelimiterSlot>,
    strong_delimiter_slots: Vec<InlineDelimiterSlot>,
    unordered_list_marker_sites: Vec<UnorderedListMarkerSite>,
    ordered_list_marker_sites: Vec<OrderedListMarkerSite>,
    thematic_break_ranges: Vec<Range<usize>>,
    heading_attr_sites: Vec<HeadingAttrSite>,
    inline_link_destination_slots: Vec<InlineLinkDestinationSlot>,
    reference_definition_sites: Vec<ReferenceDefinitionSite>,
    table_sites: Vec<TableSite>,
    wrappable_paragraphs: Vec<WrappableParagraph>,
    block_gap_sites: Vec<BlockGapSite>,
}

impl FormatFacts {
    pub(crate) fn from_parts(
        source: &str,
        events: &[(Event<'_>, Range<usize>)],
        autolinks: &[AutolinkFact],
        math_regions: &[MathRegion],
        code_blocks: &[CodeBlock],
        html_blocks: &[HtmlBlock],
        tree: &Tree,
    ) -> Self {
        Self {
            structural_spans: structural_spans(events),
            emphasis_delimiter_slots: inline_delimiter_slots(source, events, InlineDelimiterKind::Emphasis),
            strong_delimiter_slots: inline_delimiter_slots(source, events, InlineDelimiterKind::Strong),
            unordered_list_marker_sites: unordered_list_marker_sites(source, events),
            ordered_list_marker_sites: ordered_list_marker_sites(source, events),
            thematic_break_ranges: thematic_break_ranges(source, events),
            heading_attr_sites: heading_attr_sites(source, events),
            inline_link_destination_slots: inline_link_destination_slots(source, events),
            reference_definition_sites: reference_definition_sites(source, code_blocks, html_blocks),
            table_sites: table_sites(source, tree),
            wrappable_paragraphs: wrappable_paragraphs(source, events, autolinks, math_regions),
            block_gap_sites: block_gap_sites(source, tree),
        }
    }
}

impl Document {
    /// Recognised block/container ranges used as rewrite owners.
    #[must_use]
    pub fn structural_spans(&self) -> &[StructuralSpan] {
        &self.format_facts().structural_spans
    }

    /// Inline emphasis/strong delimiter slots.
    #[must_use]
    pub fn inline_delimiter_slots(&self, kind: InlineDelimiterKind) -> &[InlineDelimiterSlot] {
        match kind {
            InlineDelimiterKind::Emphasis => &self.format_facts().emphasis_delimiter_slots,
            InlineDelimiterKind::Strong => &self.format_facts().strong_delimiter_slots,
        }
    }

    /// Unordered list item marker sites.
    #[must_use]
    pub fn unordered_list_marker_sites(&self) -> &[UnorderedListMarkerSite] {
        &self.format_facts().unordered_list_marker_sites
    }

    /// Ordered list item marker digit sites.
    #[must_use]
    pub fn ordered_list_marker_sites(&self) -> &[OrderedListMarkerSite] {
        &self.format_facts().ordered_list_marker_sites
    }

    /// Thematic break source line ranges.
    #[must_use]
    pub fn thematic_break_ranges(&self) -> &[Range<usize>] {
        &self.format_facts().thematic_break_ranges
    }

    /// Heading attribute trailer sites.
    #[must_use]
    pub fn heading_attr_sites(&self) -> &[HeadingAttrSite] {
        &self.format_facts().heading_attr_sites
    }

    /// Inline link/image destination slots.
    #[must_use]
    pub fn inline_link_destination_slots(&self) -> &[InlineLinkDestinationSlot] {
        &self.format_facts().inline_link_destination_slots
    }

    /// Reference-definition destination ranges.
    #[must_use]
    pub fn reference_definition_sites(&self) -> &[ReferenceDefinitionSite] {
        &self.format_facts().reference_definition_sites
    }

    /// GFM table source rows and cell ranges.
    #[must_use]
    pub fn table_sites(&self) -> &[TableSite] {
        &self.format_facts().table_sites
    }

    /// Paragraph ranges and inline atomics for the wrap pass.
    #[must_use]
    pub fn wrappable_paragraphs(&self) -> &[WrappableParagraph] {
        &self.format_facts().wrappable_paragraphs
    }

    /// Whitespace gaps between adjacent top-level blocks, in source order.
    #[must_use]
    pub fn block_gap_sites(&self) -> &[BlockGapSite] {
        &self.format_facts().block_gap_sites
    }
}

fn structural_spans(events: &[(Event<'_>, Range<usize>)]) -> Vec<StructuralSpan> {
    let mut out = Vec::new();
    for (event, range) in events {
        match event {
            Event::Start(Tag::Paragraph) => out.push(StructuralSpan {
                kind: StructuralKind::Paragraph,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::Heading { .. }) => out.push(StructuralSpan {
                kind: StructuralKind::Heading,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::BlockQuote(_)) => out.push(StructuralSpan {
                kind: StructuralKind::BlockQuote,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::List(_)) => out.push(StructuralSpan {
                kind: StructuralKind::List,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::Item) => out.push(StructuralSpan {
                kind: StructuralKind::ListItem,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::FootnoteDefinition(_)) => out.push(StructuralSpan {
                kind: StructuralKind::FootnoteDefinition,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::Table(_)) => out.push(StructuralSpan {
                kind: StructuralKind::Table,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::DefinitionList) => out.push(StructuralSpan {
                kind: StructuralKind::DefinitionList,
                raw_range: range.clone(),
            }),
            Event::Start(Tag::DefinitionListDefinition) => out.push(StructuralSpan {
                kind: StructuralKind::DefinitionDescription,
                raw_range: range.clone(),
            }),
            Event::Rule => out.push(StructuralSpan {
                kind: StructuralKind::ThematicBreak,
                raw_range: range.clone(),
            }),
            _ => {}
        }
    }
    out
}

fn inline_delimiter_slots(
    source: &str,
    events: &[(Event<'_>, Range<usize>)],
    kind: InlineDelimiterKind,
) -> Vec<InlineDelimiterSlot> {
    let mut starts: Vec<usize> = Vec::new();
    let mut slots: Vec<InlineDelimiterSlot> = Vec::new();
    let delim_len = match kind {
        InlineDelimiterKind::Emphasis => 1,
        InlineDelimiterKind::Strong => 2,
    };
    let bytes = source.as_bytes();
    for (ev, range) in events {
        if delimiter_matches_start(ev, kind) {
            starts.push(range.start);
        } else if delimiter_matches_end(ev, kind) {
            let Some(open_lo) = starts.pop() else { continue };
            let close_hi = range.end;
            if close_hi < delim_len {
                continue;
            }
            let close_lo = close_hi.saturating_sub(delim_len);
            let open_hi = open_lo.saturating_add(delim_len);
            if open_hi > close_lo {
                continue;
            }
            let Some(open) = bytes.get(open_lo..open_hi) else {
                continue;
            };
            let Some(close) = bytes.get(close_lo..close_hi) else {
                continue;
            };
            if !is_emphasis_delim_run(open) || !is_emphasis_delim_run(close) {
                continue;
            }
            slots.push(InlineDelimiterSlot {
                pair: slots.len(),
                kind,
                open_lo,
                open_hi,
                close_lo,
                close_hi,
            });
        }
    }
    slots
}

fn unordered_list_marker_sites(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<UnorderedListMarkerSite> {
    let bytes = source.as_bytes();
    let mut stack: Vec<bool> = Vec::new();
    let mut completed = Vec::new();
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::List(start)) => {
                stack.push(start.is_none());
            }
            Event::End(TagEnd::List(_)) => {
                stack.pop();
            }
            Event::Start(Tag::Item) => {
                let Some(unordered) = stack.last().copied() else {
                    continue;
                };
                if unordered && let Some(marker) = find_unordered_bullet(bytes, range.start, range.end) {
                    completed.push(UnorderedListMarkerSite { marker });
                }
            }
            _ => {}
        }
    }
    completed
}

#[derive(Clone, Debug)]
struct OrderedListFrame {
    start_number: u64,
    next_ordinal: usize,
}

fn ordered_list_marker_sites(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<OrderedListMarkerSite> {
    let bytes = source.as_bytes();
    let mut stack: Vec<Option<OrderedListFrame>> = Vec::new();
    let mut completed = Vec::new();
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::List(start)) => {
                stack.push(start.map(|start_number| OrderedListFrame {
                    start_number,
                    next_ordinal: 0,
                }));
            }
            Event::End(TagEnd::List(_)) => {
                stack.pop();
            }
            Event::Start(Tag::Item) => {
                let Some(Some(frame)) = stack.last_mut() else {
                    continue;
                };
                if let Some((marker_lo, marker_hi)) = find_ordered_marker_digits(bytes, range.start, range.end) {
                    completed.push(OrderedListMarkerSite {
                        marker_lo,
                        marker_hi,
                        start_number: frame.start_number,
                        ordinal: frame.next_ordinal,
                    });
                    frame.next_ordinal = frame.next_ordinal.saturating_add(1);
                }
            }
            _ => {}
        }
    }
    completed
}

fn thematic_break_ranges(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<Range<usize>> {
    let mut sites = Vec::new();
    let bytes = source.as_bytes();
    for (ev, range) in events {
        if matches!(ev, Event::Rule) {
            let mut hi = range.end.min(bytes.len());
            while hi > range.start
                && matches!(
                    bytes.get(hi.saturating_sub(1)).copied(),
                    Some(b' ' | b'\t' | 0x0c | b'\n' | b'\r')
                )
            {
                hi = hi.saturating_sub(1);
            }
            sites.push(range.start..hi);
        }
    }
    sites
}

fn heading_attr_sites(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<HeadingAttrSite> {
    let mut sites = Vec::new();
    for (ev, range) in events {
        if let Event::Start(Tag::Heading { id, classes, attrs, .. }) = ev
            && (id.is_some() || !classes.is_empty() || !attrs.is_empty())
        {
            let heading_attrs = HeadingAttrs {
                id: id.as_ref().map(std::string::ToString::to_string),
                classes: classes.iter().map(std::string::ToString::to_string).collect(),
                attrs: attrs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.as_ref().map(std::string::ToString::to_string)))
                    .collect(),
                source_trailer: String::new(),
            };
            let Some(slice) = source.get(range.clone()) else {
                continue;
            };
            if let Some(trailer) = find_attr_trailer_range(slice) {
                sites.push(HeadingAttrSite {
                    attrs: heading_attrs,
                    trailer: range.start.saturating_add(trailer.start)..range.start.saturating_add(trailer.end),
                });
            }
        }
    }
    sites
}

fn inline_link_destination_slots(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<InlineLinkDestinationSlot> {
    let bytes = source.as_bytes();
    let mut sites = Vec::new();
    let mut link_stack = Vec::new();
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => link_stack.push(range.start),
            Event::End(TagEnd::Link | TagEnd::Image) => {
                let Some(open) = link_stack.pop() else { continue };
                if let Some((lo, hi)) = find_inline_dest_range(bytes, open, range.end) {
                    sites.push(InlineLinkDestinationSlot { range: lo..hi });
                }
            }
            _ => {}
        }
    }
    sites
}

fn reference_definition_sites(
    source: &str,
    code_blocks: &[CodeBlock],
    html_blocks: &[HtmlBlock],
) -> Vec<ReferenceDefinitionSite> {
    let excluded = excluded_block_ranges(code_blocks, html_blocks);
    let mut seen = std::collections::HashSet::new();
    let bytes = source.as_bytes();
    let mut sites = Vec::new();
    let mut line_start = 0usize;
    while line_start <= bytes.len() {
        let line_end = bytes
            .get(line_start..)
            .and_then(|tail| tail.iter().position(|&b| b == b'\n'))
            .map_or(bytes.len(), |p| line_start.saturating_add(p));
        if !range_start_is_excluded(line_start, &excluded)
            && let Some(site) = parse_ref_def_line(bytes, line_start, line_end)
            && let Some(norm) = NormalisedLabel::from_raw(&site.label)
            && seen.insert(norm)
        {
            sites.push(ReferenceDefinitionSite {
                raw_range: line_start..line_end,
                destination: site.dest,
            });
        }
        if line_end == bytes.len() {
            break;
        }
        line_start = line_end.saturating_add(1);
    }
    sites
}

fn table_sites(source: &str, tree: &Tree) -> Vec<TableSite> {
    let mut sites = Vec::new();
    for id in tree.descendants(tree.root()) {
        let Some(node) = tree.node(id) else { continue };
        let NodeKind::Table { alignments } = &node.kind else {
            continue;
        };
        let rows = table_rows(source, node.raw_range.clone());
        if rows.len() >= 2 {
            sites.push(TableSite {
                raw_range: node.raw_range.clone(),
                alignments: alignments.clone(),
                rows,
            });
        }
    }
    sites
}

fn block_gap_sites(source: &str, tree: &Tree) -> Vec<BlockGapSite> {
    let bytes = source.as_bytes();
    let children: Vec<_> = tree.children(tree.root()).collect();
    let mut sites = Vec::new();
    for (prev_id, next_id) in children.iter().zip(children.iter().skip(1)) {
        let (Some(prev), Some(next)) = (tree.node(*prev_id), tree.node(*next_id)) else {
            continue;
        };
        let Some(end) = gap_end(bytes, next.raw_range.start) else {
            continue;
        };
        let start = blank_run_start(bytes, end);
        // `prev` must reach the gap. A construct the tree does not model
        // — a link reference definition, say — occupies a non-blank line
        // that stops the backward walk while `prev` ends far above it,
        // and normalising around it would be normalising around the
        // wrong block.
        if prev.raw_range.end > end || start.saturating_sub(1) > prev.raw_range.end {
            continue;
        }
        let Some(gap) = source.get(start..end) else {
            continue;
        };
        if !gap.bytes().all(|byte| matches!(byte, b' ' | b'\t' | b'\n')) {
            continue;
        }
        sites.push(BlockGapSite {
            range: start..end,
            prev_is_heading: matches!(prev.kind, NodeKind::Heading { .. }),
            next_is_heading: matches!(next.kind, NodeKind::Heading { .. }),
        });
    }
    sites
}

/// Start of the run of blank lines ending at the line start `line_start`.
///
/// The gap's left edge is found by walking back over blank lines rather
/// than read off the preceding block's `raw_range.end`. A block's end
/// offset may sit before its own terminating newline, after it, or —
/// for a list — past a following blank line entirely, and a gap measured
/// from the overshooting end is empty, so normalising it would insert a
/// blank line on every pass and never converge. A block's last line is
/// never blank, so the backward walk stops exactly at its content.
fn blank_run_start(bytes: &[u8], line_start: usize) -> usize {
    let mut start = line_start;
    while start > 0 {
        if bytes.get(start.wrapping_sub(1)).copied() != Some(b'\n') {
            break;
        }
        let line_end = start.wrapping_sub(1);
        let mut above = line_end;
        while above > 0 && bytes.get(above.wrapping_sub(1)).copied() != Some(b'\n') {
            above = above.wrapping_sub(1);
        }
        let blank = (above..line_end).all(|idx| matches!(bytes.get(idx).copied(), Some(b' ' | b'\t')));
        if !blank {
            break;
        }
        start = above;
    }
    start
}

/// Start of the line the block at `next_start` begins on.
///
/// Returns `None` when anything other than indentation separates the
/// line start from `next_start`, which would mean the two blocks share a
/// line and the span between them is not a gap.
fn gap_end(bytes: &[u8], next_start: usize) -> Option<usize> {
    let mut idx = next_start;
    while idx > 0 {
        match bytes.get(idx.wrapping_sub(1)).copied() {
            Some(b'\n') => return Some(idx),
            Some(b' ' | b'\t') => idx = idx.wrapping_sub(1),
            _ => return None,
        }
    }
    Some(0)
}

fn wrappable_paragraphs(
    source: &str,
    events: &[(Event<'_>, Range<usize>)],
    autolinks: &[AutolinkFact],
    math_regions: &[MathRegion],
) -> Vec<WrappableParagraph> {
    let mut paragraphs = Vec::new();
    let bytes = source.as_bytes();
    let mut current: Option<PartialParagraph> = None;
    let mut paragraph_depth: u32 = 0;
    let mut prose_container_depth: u32 = 0;

    for (ev, range) in events {
        match ev {
            Event::Start(Tag::Paragraph) => {
                if paragraph_depth == 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                paragraph_depth = paragraph_depth.saturating_add(1);
            }
            Event::End(TagEnd::Paragraph) => {
                paragraph_depth = paragraph_depth.saturating_sub(1);
                if paragraph_depth == 0
                    && let Some(p) = current.take()
                    && let Some(finished) = p.finish(bytes, autolinks, math_regions)
                {
                    paragraphs.push(finished);
                }
            }
            Event::Start(Tag::Item | Tag::DefinitionListDefinition | Tag::FootnoteDefinition(_)) => {
                prose_container_depth = prose_container_depth.saturating_add(1);
            }
            Event::End(TagEnd::Item | TagEnd::DefinitionListDefinition | TagEnd::FootnoteDefinition) => {
                prose_container_depth = prose_container_depth.saturating_sub(1);
                if let Some(p) = current.take()
                    && let Some(finished) = p.finish(bytes, autolinks, math_regions)
                {
                    paragraphs.push(finished);
                }
            }
            Event::Start(
                Tag::CodeBlock(_)
                | Tag::HtmlBlock
                | Tag::Heading { .. }
                | Tag::BlockQuote(_)
                | Tag::List(_)
                | Tag::Table(_)
                | Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::MetadataBlock(_),
            ) => {
                if let Some(p) = current.take()
                    && let Some(finished) = p.finish(bytes, autolinks, math_regions)
                {
                    paragraphs.push(finished);
                }
            }
            Event::Text(_) => {
                if current.is_none() && paragraph_depth == 0 && prose_container_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::Code(_) | Event::InlineHtml(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {
                if current.is_none() && paragraph_depth == 0 && prose_container_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut() {
                    p.atomics.push(range.clone());
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            Event::SoftBreak => {
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) => {
                if current.is_none() && paragraph_depth == 0 && prose_container_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut() {
                    p.link_stack.push(range.start);
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                if let Some(p) = current.as_mut() {
                    if let Some(start) = p.link_stack.pop() {
                        p.atomics.push(start..range.end);
                    }
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            Event::Start(Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Superscript | Tag::Subscript) => {
                if current.is_none() && paragraph_depth == 0 && prose_container_depth > 0 {
                    current = Some(PartialParagraph::new(range.clone()));
                }
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::End(
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Superscript | TagEnd::Subscript,
            ) => {
                if let Some(p) = current.as_mut()
                    && range.end > p.content_hi
                {
                    p.content_hi = range.end;
                }
            }
            Event::HardBreak => {
                if let Some(p) = current.as_mut() {
                    if let Some(hb) = classify_hard_break(bytes, range.start, range.end) {
                        p.hard_breaks.push(hb);
                    }
                    if range.end > p.content_hi {
                        p.content_hi = range.end;
                    }
                }
            }
            _ => {}
        }
    }
    paragraphs
}

fn excluded_block_ranges(code_blocks: &[CodeBlock], html_blocks: &[HtmlBlock]) -> Vec<Range<usize>> {
    code_blocks
        .iter()
        .map(|b| b.raw_range.clone())
        .chain(html_blocks.iter().map(|b| b.raw_range.clone()))
        .collect()
}

fn delimiter_matches_start(ev: &Event<'_>, kind: InlineDelimiterKind) -> bool {
    match kind {
        InlineDelimiterKind::Emphasis => matches!(ev, Event::Start(Tag::Emphasis)),
        InlineDelimiterKind::Strong => matches!(ev, Event::Start(Tag::Strong)),
    }
}

fn delimiter_matches_end(ev: &Event<'_>, kind: InlineDelimiterKind) -> bool {
    match kind {
        InlineDelimiterKind::Emphasis => matches!(ev, Event::End(TagEnd::Emphasis)),
        InlineDelimiterKind::Strong => matches!(ev, Event::End(TagEnd::Strong)),
    }
}

fn is_emphasis_delim_run(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|&b| b == b'*' || b == b'_')
}

fn find_unordered_bullet(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    let end = end.min(bytes.len());
    let mut i = start;
    while i < end {
        let b = bytes.get(i).copied()?;
        if b == b'-' || b == b'*' || b == b'+' {
            return Some(i);
        }
        if b != b' ' && b != b'\t' {
            return None;
        }
        i = i.saturating_add(1);
    }
    None
}

fn table_rows(source: &str, range: Range<usize>) -> Vec<TableRowSite> {
    let mut rows = Vec::new();
    let bytes = source.as_bytes();
    let mut line_start = range.start.min(bytes.len());
    let range_end = range.end.min(bytes.len());
    while line_start < range_end {
        let line_end = bytes
            .get(line_start..range_end)
            .and_then(|tail| tail.iter().position(|&b| b == b'\n'))
            .map_or(range_end, |p| line_start.saturating_add(p));
        let raw_end = if line_end > line_start && bytes.get(line_end.saturating_sub(1)) == Some(&b'\r') {
            line_end.saturating_sub(1)
        } else {
            line_end
        };
        if let Some(row) = table_row(source, line_start..raw_end) {
            rows.push(row);
        }
        if line_end == range_end {
            break;
        }
        line_start = line_end.saturating_add(1);
    }
    rows
}

fn table_row(source: &str, range: Range<usize>) -> Option<TableRowSite> {
    let bytes = source.as_bytes();
    let line = bytes.get(range.clone())?;
    let mut lo = range.start;
    let mut hi = range.end;
    while lo < hi && bytes.get(lo).is_some_and(u8::is_ascii_whitespace) {
        lo = lo.saturating_add(1);
    }
    while hi > lo && bytes.get(hi.saturating_sub(1)).is_some_and(u8::is_ascii_whitespace) {
        hi = hi.saturating_sub(1);
    }
    if lo < hi && bytes.get(lo) == Some(&b'|') {
        lo = lo.saturating_add(1);
    }
    if hi > lo && bytes.get(hi.saturating_sub(1)) == Some(&b'|') {
        hi = hi.saturating_sub(1);
    }
    let mut cells = Vec::new();
    let mut cell_start = lo;
    let mut i = lo;
    let mut escaped = false;
    while i < hi {
        let Some(b) = bytes.get(i).copied() else {
            break;
        };
        if b == b'|' && !escaped {
            cells.push(TableCellSite {
                raw_range: cell_start..i,
            });
            cell_start = i.saturating_add(1);
        }
        escaped = b == b'\\' && !escaped;
        if b != b'\\' {
            escaped = false;
        }
        i = i.saturating_add(1);
    }
    cells.push(TableCellSite {
        raw_range: cell_start..hi,
    });
    if cells.is_empty() || !line.contains(&b'|') {
        return None;
    }
    Some(TableRowSite {
        raw_range: range,
        cells,
    })
}

fn find_ordered_marker_digits(bytes: &[u8], start: usize, end: usize) -> Option<(usize, usize)> {
    let end = end.min(bytes.len());
    let mut i = start;
    while i < end {
        let b = bytes.get(i).copied()?;
        if b == b' ' || b == b'\t' {
            i = i.saturating_add(1);
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        let digit_lo = i;
        while i < end && bytes.get(i).copied().is_some_and(|c| c.is_ascii_digit()) {
            i = i.saturating_add(1);
        }
        return Some((digit_lo, i));
    }
    None
}

fn find_inline_dest_range(bytes: &[u8], start: usize, end: usize) -> Option<(usize, usize)> {
    let end = end.min(bytes.len());
    let bracket = if bytes.get(start).copied()? == b'!' {
        start.saturating_add(1)
    } else {
        start
    };
    if bytes.get(bracket).copied()? != b'[' {
        return None;
    }
    let mut depth: i32 = 1;
    let mut i = bracket.saturating_add(1);
    while i < end {
        let b = bytes.get(i).copied()?;
        match b {
            b'\\' => {
                i = i.saturating_add(2);
                continue;
            }
            b'[' => depth = depth.saturating_add(1),
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i = i.saturating_add(1);
    }
    if depth != 0 || bytes.get(i).copied() != Some(b']') {
        return None;
    }
    let after_close = i.saturating_add(1);
    if bytes.get(after_close).copied() != Some(b'(') {
        return None;
    }
    let mut j = after_close.saturating_add(1);
    while j < end && matches!(bytes.get(j).copied(), Some(b' ' | b'\t' | b'\n')) {
        j = j.saturating_add(1);
    }
    let dest_lo = j;
    let dest_hi = if bytes.get(j).copied() == Some(b'<') {
        let mut k = j.saturating_add(1);
        while k < end && bytes.get(k).copied() != Some(b'>') {
            if bytes.get(k).copied() == Some(b'\n') {
                return None;
            }
            k = k.saturating_add(1);
        }
        if bytes.get(k).copied() != Some(b'>') {
            return None;
        }
        k.saturating_add(1)
    } else {
        let mut depth: i32 = 0;
        let mut k = j;
        while k < end {
            let b = bytes.get(k).copied()?;
            match b {
                b'\\' => {
                    k = k.saturating_add(2);
                    continue;
                }
                b'(' => depth = depth.saturating_add(1),
                b')' => {
                    if depth == 0 {
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                b' ' | b'\t' | b'\n' => break,
                _ => {}
            }
            k = k.saturating_add(1);
        }
        k
    };
    if dest_hi <= dest_lo {
        return None;
    }
    Some((dest_lo, dest_hi))
}

fn range_start_is_excluded(start: usize, excluded: &[Range<usize>]) -> bool {
    excluded.iter().any(|r| r.start <= start && start < r.end)
}

struct RefDefSite {
    label: String,
    dest: Range<usize>,
}

fn parse_ref_def_line(bytes: &[u8], lo: usize, hi: usize) -> Option<RefDefSite> {
    let mut i = lo;
    let mut spaces = 0usize;
    while i < hi && bytes.get(i).copied() == Some(b' ') && spaces < 3 {
        i = i.saturating_add(1);
        spaces = spaces.saturating_add(1);
    }
    if bytes.get(i).copied() != Some(b'[') {
        return None;
    }
    i = i.saturating_add(1);
    let label_lo = i;
    while i < hi {
        let b = bytes.get(i).copied()?;
        match b {
            b'\\' => i = i.saturating_add(2),
            b']' => break,
            b'\n' => return None,
            _ => i = i.saturating_add(1),
        }
    }
    let label_hi = i;
    if bytes.get(i).copied() != Some(b']') {
        return None;
    }
    i = i.saturating_add(1);
    if bytes.get(i).copied() != Some(b':') {
        return None;
    }
    i = i.saturating_add(1);
    while i < hi && matches!(bytes.get(i).copied(), Some(b' ' | b'\t')) {
        i = i.saturating_add(1);
    }
    if i >= hi {
        return None;
    }
    let dest_lo = i;
    let dest_hi = if bytes.get(i).copied() == Some(b'<') {
        let mut k = i.saturating_add(1);
        while k < hi && bytes.get(k).copied() != Some(b'>') {
            k = k.saturating_add(1);
        }
        if bytes.get(k).copied() != Some(b'>') {
            return None;
        }
        k.saturating_add(1)
    } else {
        let mut k = i;
        while k < hi && !matches!(bytes.get(k).copied(), Some(b' ' | b'\t')) {
            k = k.saturating_add(1);
        }
        k
    };
    if dest_hi <= dest_lo {
        return None;
    }
    let label = std::str::from_utf8(bytes.get(label_lo..label_hi)?).ok()?.to_owned();
    Some(RefDefSite {
        label,
        dest: dest_lo..dest_hi,
    })
}

struct PartialParagraph {
    content_lo: usize,
    content_hi: usize,
    atomics: Vec<Range<usize>>,
    hard_breaks: Vec<ParagraphHardBreak>,
    link_stack: Vec<usize>,
}

impl PartialParagraph {
    fn new(range: Range<usize>) -> Self {
        Self {
            content_lo: range.start,
            content_hi: range.end,
            atomics: Vec::new(),
            hard_breaks: Vec::new(),
            link_stack: Vec::new(),
        }
    }

    fn finish(
        mut self,
        bytes: &[u8],
        extra_atomics: &[crate::AutolinkFact],
        math_regions: &[MathRegion],
    ) -> Option<WrappableParagraph> {
        let (line_lo, first_prefix) = extract_first_prefix(bytes, self.content_lo)?;
        let line_hi = extract_line_hi(bytes, self.content_hi);
        if is_mkdocs_admonition_paragraph(bytes, line_lo, line_hi) {
            return None;
        }
        let cont_prefix = derive_continuation_prefix(&first_prefix)?;
        let list_four_space_cont_prefix = derive_list_four_space_continuation_prefix(&first_prefix);
        let owner_kind = paragraph_owner_kind(&first_prefix);
        for autolink in extra_atomics {
            let raw_range = autolink.raw_range();
            if raw_range.start >= self.content_lo && raw_range.end <= self.content_hi {
                self.atomics.push(raw_range);
            }
        }
        for region in math_regions {
            if region.range.start >= self.content_lo && region.range.end <= self.content_hi {
                self.atomics.push(region.range.clone());
            }
        }
        let mut atomics = self.atomics;
        atomics.sort_by_key(|r| r.start);
        let mut hard_breaks = self.hard_breaks;
        hard_breaks.sort_by_key(|h| h.nl);
        Some(WrappableParagraph {
            line_lo,
            line_hi,
            content_lo: self.content_lo,
            content_hi: self.content_hi,
            owner_kind,
            first_prefix,
            cont_prefix,
            list_four_space_cont_prefix,
            atomics,
            hard_breaks,
        })
    }
}

fn paragraph_owner_kind(first_prefix: &str) -> StructuralKind {
    let trimmed = first_prefix.trim_start_matches([' ', '\t']);
    if trimmed.starts_with('>') {
        StructuralKind::BlockQuote
    } else if trimmed.starts_with("[^") {
        StructuralKind::FootnoteDefinition
    } else if trimmed.starts_with(':') {
        StructuralKind::DefinitionDescription
    } else if trimmed.starts_with(['-', '*', '+']) || trimmed.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        StructuralKind::ListItem
    } else {
        StructuralKind::Paragraph
    }
}

fn is_mkdocs_admonition_paragraph(bytes: &[u8], line_lo: usize, line_hi: usize) -> bool {
    let Some(line) = bytes.get(line_lo..line_hi) else {
        return false;
    };
    let first_line_end = line.iter().position(|&b| b == b'\n').unwrap_or(line.len());
    let Some(first_line) = line.get(..first_line_end) else {
        return false;
    };
    let indent = first_line.iter().take_while(|&&b| b == b' ').count();
    if indent > 3 {
        return false;
    }
    let marker = first_line.get(indent..).unwrap_or(&[]);
    is_admonition_marker(marker, b"!!!") || is_admonition_marker(marker, b"???")
}

fn is_admonition_marker(line: &[u8], opener: &[u8]) -> bool {
    let Some(after_opener) = line.get(opener.len()..) else {
        return false;
    };
    if !line.starts_with(opener) {
        return false;
    }
    match after_opener.first().copied() {
        Some(b' ' | b'\t') => true,
        Some(b'+' | b'-') => matches!(after_opener.get(1).copied(), Some(b' ' | b'\t')),
        _ => false,
    }
}

fn classify_hard_break(bytes: &[u8], start: usize, end: usize) -> Option<ParagraphHardBreak> {
    let slice = bytes.get(start..end)?;
    let nl_off = slice.iter().rposition(|&b| b == b'\n')?;
    let nl = start.saturating_add(nl_off);
    let before_nl = bytes.get(nl.checked_sub(1)?).copied()?;
    if before_nl == b'\\' {
        let two_back = nl.checked_sub(2).and_then(|i| bytes.get(i).copied());
        if matches!(two_back, Some(b'\\')) {
            return None;
        }
        return Some(ParagraphHardBreak {
            marker_lo: nl.saturating_sub(1),
            nl,
            marker: "\\",
        });
    }
    if before_nl == b' ' {
        let two_back = nl.checked_sub(2).and_then(|i| bytes.get(i).copied());
        if matches!(two_back, Some(b' ')) {
            return Some(ParagraphHardBreak {
                marker_lo: nl.saturating_sub(2),
                nl,
                marker: "  ",
            });
        }
    }
    None
}

fn extract_first_prefix(bytes: &[u8], content_lo: usize) -> Option<(usize, String)> {
    let line_lo = bytes
        .get(..content_lo)?
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |p| p.saturating_add(1));
    let prefix = bytes.get(line_lo..content_lo)?;
    let s = std::str::from_utf8(prefix).ok()?.to_owned();
    Some((line_lo, s))
}

fn extract_line_hi(bytes: &[u8], content_hi: usize) -> usize {
    let len = bytes.len();
    let content_hi = content_hi.min(len);
    if content_hi > 0 && bytes.get(content_hi.saturating_sub(1)).copied() == Some(b'\n') {
        return content_hi;
    }
    let Some(tail) = bytes.get(content_hi..) else {
        return len;
    };
    tail.iter()
        .position(|&b| b == b'\n')
        .map_or(len, |p| content_hi.saturating_add(p).saturating_add(1))
}

fn derive_continuation_prefix(first: &str) -> Option<String> {
    let bytes = first.as_bytes();
    let mut out = String::with_capacity(first.len());
    let mut i = 0usize;
    while let Some(b) = bytes.get(i).copied() {
        match b {
            b'>' => {
                out.push('>');
                i = i.saturating_add(1);
                if bytes.get(i).copied() == Some(b' ') {
                    out.push(' ');
                    i = i.saturating_add(1);
                }
            }
            b' ' | b'\t' => {
                out.push(b as char);
                i = i.saturating_add(1);
            }
            b'-' | b'*' | b'+' => {
                out.push(' ');
                i = i.saturating_add(1);
                if bytes.get(i).copied() == Some(b' ') {
                    out.push(' ');
                    i = i.saturating_add(1);
                }
            }
            b'0'..=b'9' => {
                let start = i;
                while bytes.get(i).copied().is_some_and(|c| c.is_ascii_digit()) {
                    i = i.saturating_add(1);
                }
                if matches!(bytes.get(i).copied(), Some(b'.' | b')')) {
                    i = i.saturating_add(1);
                }
                if bytes.get(i).copied() == Some(b' ') {
                    i = i.saturating_add(1);
                }
                for _ in 0..i.saturating_sub(start) {
                    out.push(' ');
                }
            }
            b'[' if bytes.get(i.saturating_add(1)).copied() == Some(b'^') => {
                i = i.saturating_add(2);
                let mut closed = false;
                while let Some(c) = bytes.get(i).copied() {
                    i = i.saturating_add(1);
                    if c == b']' && bytes.get(i).copied() == Some(b':') {
                        i = i.saturating_add(1);
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return None;
                }
                while bytes.get(i).copied().is_some_and(|c| matches!(c, b' ' | b'\t')) {
                    i = i.saturating_add(1);
                }
                out.push_str("    ");
            }
            b':' => {
                let start = i;
                i = i.saturating_add(1);
                while bytes.get(i).copied().is_some_and(|c| matches!(c, b' ' | b'\t')) {
                    i = i.saturating_add(1);
                }
                if i == start.saturating_add(1) {
                    return None;
                }
                for _ in 0..i.saturating_sub(start) {
                    out.push(' ');
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

fn derive_list_four_space_continuation_prefix(first: &str) -> Option<String> {
    let bytes = first.as_bytes();
    let mut out = String::with_capacity(first.len().saturating_add(2));
    let mut i = 0usize;
    while let Some(b) = bytes.get(i).copied() {
        match b {
            b'>' => {
                out.push('>');
                i = i.saturating_add(1);
                if bytes.get(i).copied() == Some(b' ') {
                    out.push(' ');
                    i = i.saturating_add(1);
                }
            }
            b' ' | b'\t' => {
                out.push(b as char);
                i = i.saturating_add(1);
            }
            b'-' | b'*' | b'+' => {
                i = i.saturating_add(1);
                let _has_marker_space = bytes.get(i).copied() == Some(b' ');
                out.push_str("    ");
                return Some(out);
            }
            b'0'..=b'9' => {
                while bytes.get(i).copied().is_some_and(|c| c.is_ascii_digit()) {
                    i = i.saturating_add(1);
                }
                if !matches!(bytes.get(i).copied(), Some(b'.' | b')')) {
                    return None;
                }
                i = i.saturating_add(1);
                let _has_marker_space = bytes.get(i).copied() == Some(b' ');
                out.push_str("    ");
                return Some(out);
            }
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "fact tests assert a specific recognised paragraph exists"
)]
mod tests {
    use super::*;

    #[test]
    fn footnote_definition_continuation_uses_four_space_indent() {
        let doc = Document::parse("[^long-label]: alpha beta gamma\n").expect("fixture parses");
        let paragraph = doc
            .wrappable_paragraphs()
            .iter()
            .next()
            .expect("footnote definition paragraph");
        assert_eq!(paragraph.cont_prefix, "    ");
    }

    #[test]
    fn definition_list_continuation_uses_marker_width_indent() {
        let doc = Document::parse("term\n:   alpha beta gamma\n").expect("fixture parses");
        let paragraph = doc
            .wrappable_paragraphs()
            .iter()
            .next()
            .expect("definition list paragraph");
        assert_eq!(paragraph.cont_prefix, "    ");
    }

    #[test]
    fn list_paragraph_exposes_four_space_continuation_prefix() {
        let doc = Document::parse("> - alpha beta gamma\n").expect("fixture parses");
        let paragraph = doc.wrappable_paragraphs().iter().next().expect("list paragraph");
        assert_eq!(paragraph.cont_prefix, ">   ");
        assert_eq!(paragraph.list_four_space_cont_prefix.as_deref(), Some(">     "));
    }

    #[test]
    fn unordered_nested_list_marker_facts_are_marker_local() {
        let src = " *   * * *   * *   * *   *   * \\\\*";
        let doc = Document::parse(src).expect("fixture parses");
        let markers: Vec<_> = doc
            .unordered_list_marker_sites()
            .iter()
            .map(UnorderedListMarkerSite::marker_range)
            .collect();
        assert_eq!(
            markers,
            vec![1..2, 5..6, 7..8, 9..10, 13..14, 15..16, 19..20, 21..22, 25..26, 29..30]
        );
    }

    #[test]
    fn ordered_list_marker_facts_carry_list_start_and_ordinal() {
        let doc = Document::parse("3. alpha\n4. beta\n").expect("fixture parses");
        let markers: Vec<_> = doc
            .ordered_list_marker_sites()
            .iter()
            .map(|site| (site.marker_range(), site.start_number(), site.ordinal()))
            .collect();
        assert_eq!(markers, vec![(0..1, 3, 0), (9..10, 3, 1)]);
    }

    #[test]
    fn inline_delimiter_slots_are_pair_local() {
        let doc = Document::parse("__outer _inner___\n").expect("fixture parses");
        let strong: Vec<_> = doc
            .inline_delimiter_slots(InlineDelimiterKind::Strong)
            .iter()
            .map(|slot| (slot.pair(), slot.kind(), slot.open_range(), slot.close_range()))
            .collect();
        let emphasis: Vec<_> = doc
            .inline_delimiter_slots(InlineDelimiterKind::Emphasis)
            .iter()
            .map(|slot| (slot.pair(), slot.kind(), slot.open_range(), slot.close_range()))
            .collect();

        assert_eq!(strong, vec![(0, InlineDelimiterKind::Strong, 0..2, 15..17)]);
        assert_eq!(emphasis, vec![(0, InlineDelimiterKind::Emphasis, 8..9, 14..15)]);
    }

    #[test]
    fn inline_link_destination_slots_include_links_and_images() {
        let doc =
            Document::parse("[x](https://example.com) ![alt](https://example.com/img)\n").expect("fixture parses");
        let slots: Vec<_> = doc
            .inline_link_destination_slots()
            .iter()
            .map(InlineLinkDestinationSlot::range)
            .collect();

        assert_eq!(slots, vec![4..23, 32..55]);
    }

    #[test]
    fn table_cell_facts_preserve_escaped_pipes_inside_cells() {
        let doc =
            Document::parse("| code | escaped |\n| --- | --- |\n| `a\\|b` | left\\|right |\n").expect("fixture parses");
        let table = doc.table_sites().first().expect("table fact");
        let body = table.rows().get(2).expect("body row");
        let cells: Vec<_> = body.cells().iter().map(TableCellSite::raw_range).collect();

        assert_eq!(cells.len(), 2);
        let first = cells.first().expect("first cell");
        let second = cells.get(1).expect("second cell");
        assert_eq!(doc.source().get(first.clone()), Some(" `a\\|b` "));
        assert_eq!(doc.source().get(second.clone()), Some(" left\\|right "));
    }

    fn gaps(source: &str) -> Vec<(Range<usize>, bool, bool)> {
        let doc = Document::parse(source).expect("fixture parses");
        doc.block_gap_sites()
            .iter()
            .map(|site| (site.range(), site.prev_is_heading(), site.next_is_heading()))
            .collect()
    }

    #[test]
    fn block_gap_between_heading_and_paragraph_is_empty_when_adjacent() {
        assert_eq!(gaps("# A\nB\n"), vec![(4..4, true, false)]);
    }

    #[test]
    fn block_gap_spans_every_blank_line_between_blocks() {
        assert_eq!(gaps("# A\n\n\nB\n"), vec![(4..6, true, false)]);
    }

    #[test]
    fn block_gap_covers_blank_lines_carrying_trailing_whitespace() {
        assert_eq!(gaps("# A\n  \n\t\nB\n"), vec![(4..9, true, false)]);
    }

    #[test]
    fn block_gap_between_adjacent_headings_is_recorded_once() {
        assert_eq!(gaps("# A\n## B\n"), vec![(4..4, true, true)]);
    }

    #[test]
    fn block_gap_excludes_the_following_block_indent() {
        // The paragraph's two-space indent stays outside the gap, so
        // replacing the gap cannot delete it.
        assert_eq!(gaps("# A\n\n  B\n"), vec![(4..5, true, false)]);
    }

    #[test]
    fn no_block_gap_when_a_reference_definition_sits_between_blocks() {
        // pulldown emits no node for the definition, so it falls inside
        // the computed gap and the whitespace guard suppresses the site.
        assert_eq!(gaps("# A\n\n[a]: /x\n\nB\n"), Vec::new());
    }

    #[test]
    fn block_gaps_are_root_only() {
        // The heading nested in the block quote produces no site; the
        // only gap is between the two root-level blocks.
        assert_eq!(gaps("> # A\n> B\n\nC\n"), vec![(10..11, false, false)]);
    }

    #[test]
    fn no_block_gap_before_the_first_root_child() {
        assert_eq!(gaps("# A\n"), Vec::new());
        assert_eq!(gaps("---\ntitle: t\n---\n# A\n"), Vec::new());
    }

    #[test]
    fn block_gap_after_a_list_stops_at_the_blank_line() {
        // A list's `raw_range.end` runs past the blank line that follows
        // it. Anchoring the gap on that end would report an empty gap
        // just above the heading, and normalising it would insert one
        // more blank line on every pass.
        assert_eq!(gaps("1. a\n   b\n\n## H\n"), vec![(10..11, false, true)]);
    }

    #[test]
    fn block_gap_neighbours_include_setext_headings() {
        assert_eq!(gaps("A\n---\nB\n"), vec![(6..6, true, false)]);
    }

    #[test]
    fn block_gap_recognises_code_and_html_block_neighbours() {
        assert_eq!(gaps("# A\n```\nx\n```\n"), vec![(4..4, true, false)]);
        assert_eq!(gaps("# A\n\n<div>\n</div>\n"), vec![(4..5, true, false)]);
        assert_eq!(gaps("# A\n\n    code\n"), vec![(4..5, true, false)]);
    }
}
