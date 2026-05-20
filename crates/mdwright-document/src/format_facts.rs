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

/// Delimiter byte ranges for one inline span.
#[derive(Clone, Debug)]
pub struct InlineDelimiterSpan {
    open_lo: usize,
    open_hi: usize,
    close_lo: usize,
    close_hi: usize,
}

impl InlineDelimiterSpan {
    #[must_use]
    pub fn open_range(&self) -> Range<usize> {
        self.open_lo..self.open_hi
    }

    #[must_use]
    pub fn close_range(&self) -> Range<usize> {
        self.close_lo..self.close_hi
    }

    #[must_use]
    pub fn full_range(&self) -> Range<usize> {
        self.open_lo..self.close_hi
    }
}

/// One unordered list and the byte offsets of its item markers.
#[derive(Clone, Debug)]
pub struct UnorderedListSite {
    raw_range: Range<usize>,
    bullets: Vec<usize>,
}

impl UnorderedListSite {
    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }

    #[must_use]
    pub fn bullets(&self) -> &[usize] {
        &self.bullets
    }
}

/// One ordered list and the digit ranges of its item markers.
#[derive(Clone, Debug)]
pub struct OrderedListSite {
    raw_range: Range<usize>,
    items: Vec<OrderedItemSite>,
}

impl OrderedListSite {
    #[must_use]
    pub fn raw_range(&self) -> Range<usize> {
        self.raw_range.clone()
    }

    #[must_use]
    pub fn items(&self) -> &[OrderedItemSite] {
        &self.items
    }
}

#[derive(Clone, Debug)]
pub struct OrderedItemSite {
    marker_lo: usize,
    marker_hi: usize,
}

impl OrderedItemSite {
    #[must_use]
    pub fn marker_range(&self) -> Range<usize> {
        self.marker_lo..self.marker_hi
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

/// An inline link destination byte range.
#[derive(Clone, Debug)]
pub struct InlineLinkDestinationSite {
    range: Range<usize>,
}

impl InlineLinkDestinationSite {
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

/// A paragraph range with the inline facts needed by the wrap pass.
#[derive(Clone, Debug)]
pub struct WrappableParagraph {
    line_lo: usize,
    line_hi: usize,
    content_lo: usize,
    content_hi: usize,
    first_prefix: String,
    cont_prefix: String,
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
    pub fn first_prefix(&self) -> &str {
        &self.first_prefix
    }

    #[must_use]
    pub fn cont_prefix(&self) -> &str {
        &self.cont_prefix
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
    emphasis_delimiters: Vec<InlineDelimiterSpan>,
    strong_delimiters: Vec<InlineDelimiterSpan>,
    unordered_list_sites: Vec<UnorderedListSite>,
    ordered_list_sites: Vec<OrderedListSite>,
    thematic_break_ranges: Vec<Range<usize>>,
    heading_attr_sites: Vec<HeadingAttrSite>,
    inline_link_destination_sites: Vec<InlineLinkDestinationSite>,
    reference_definition_sites: Vec<ReferenceDefinitionSite>,
    table_sites: Vec<TableSite>,
    wrappable_paragraphs: Vec<WrappableParagraph>,
}

impl FormatFacts {
    pub(crate) fn from_parts(
        source: &str,
        events: &[(Event<'_>, Range<usize>)],
        autolinks: &[AutolinkFact],
        code_blocks: &[CodeBlock],
        html_blocks: &[HtmlBlock],
        tree: &Tree,
    ) -> Self {
        Self {
            structural_spans: structural_spans(events),
            emphasis_delimiters: inline_delimiter_spans(source, events, InlineDelimiterKind::Emphasis),
            strong_delimiters: inline_delimiter_spans(source, events, InlineDelimiterKind::Strong),
            unordered_list_sites: unordered_list_sites(source, events),
            ordered_list_sites: ordered_list_sites(source, events),
            thematic_break_ranges: thematic_break_ranges(source, events),
            heading_attr_sites: heading_attr_sites(source, events),
            inline_link_destination_sites: inline_link_destination_sites(source, events),
            reference_definition_sites: reference_definition_sites(source, code_blocks, html_blocks),
            table_sites: table_sites(source, tree),
            wrappable_paragraphs: wrappable_paragraphs(source, events, autolinks),
        }
    }
}

impl Document {
    /// Recognised block/container ranges used as rewrite owners.
    #[must_use]
    pub fn structural_spans(&self) -> &[StructuralSpan] {
        &self.format_facts().structural_spans
    }

    /// Inline emphasis/strong delimiter ranges.
    #[must_use]
    pub fn inline_delimiter_spans(&self, kind: InlineDelimiterKind) -> &[InlineDelimiterSpan] {
        match kind {
            InlineDelimiterKind::Emphasis => &self.format_facts().emphasis_delimiters,
            InlineDelimiterKind::Strong => &self.format_facts().strong_delimiters,
        }
    }

    /// Unordered list marker sites.
    #[must_use]
    pub fn unordered_list_sites(&self) -> &[UnorderedListSite] {
        &self.format_facts().unordered_list_sites
    }

    /// Ordered list marker digit sites.
    #[must_use]
    pub fn ordered_list_sites(&self) -> &[OrderedListSite] {
        &self.format_facts().ordered_list_sites
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

    /// Inline link destination ranges.
    #[must_use]
    pub fn inline_link_destination_sites(&self) -> &[InlineLinkDestinationSite] {
        &self.format_facts().inline_link_destination_sites
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

fn inline_delimiter_spans(
    source: &str,
    events: &[(Event<'_>, Range<usize>)],
    kind: InlineDelimiterKind,
) -> Vec<InlineDelimiterSpan> {
    let mut starts: Vec<usize> = Vec::new();
    let mut spans: Vec<InlineDelimiterSpan> = Vec::new();
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
            spans.push(InlineDelimiterSpan {
                open_lo,
                open_hi,
                close_lo,
                close_hi,
            });
        }
    }
    spans
}

fn unordered_list_sites(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<UnorderedListSite> {
    let bytes = source.as_bytes();
    let mut stack: Vec<(bool, UnorderedListSite)> = Vec::new();
    let mut completed = Vec::new();
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::List(start)) => {
                stack.push((
                    start.is_none(),
                    UnorderedListSite {
                        raw_range: range.clone(),
                        bullets: Vec::new(),
                    },
                ));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some((unordered, sites)) = stack.pop()
                    && unordered
                {
                    completed.push(sites);
                }
            }
            Event::Start(Tag::Item) => {
                let Some((unordered, sites)) = stack.last_mut() else {
                    continue;
                };
                if *unordered && let Some(p) = find_unordered_bullet(bytes, range.start, range.end) {
                    sites.bullets.push(p);
                }
            }
            _ => {}
        }
    }
    completed
}

fn ordered_list_sites(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<OrderedListSite> {
    let bytes = source.as_bytes();
    let mut stack: Vec<(bool, OrderedListSite)> = Vec::new();
    let mut completed = Vec::new();
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::List(start)) => {
                stack.push((
                    start.is_some(),
                    OrderedListSite {
                        raw_range: range.clone(),
                        items: Vec::new(),
                    },
                ));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some((ordered, sites)) = stack.pop()
                    && ordered
                {
                    completed.push(sites);
                }
            }
            Event::Start(Tag::Item) => {
                let Some((ordered, sites)) = stack.last_mut() else {
                    continue;
                };
                if *ordered
                    && let Some((marker_lo, marker_hi)) = find_ordered_marker_digits(bytes, range.start, range.end)
                {
                    sites.items.push(OrderedItemSite { marker_lo, marker_hi });
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

fn inline_link_destination_sites(source: &str, events: &[(Event<'_>, Range<usize>)]) -> Vec<InlineLinkDestinationSite> {
    let bytes = source.as_bytes();
    let mut sites = Vec::new();
    let mut link_stack = Vec::new();
    for (ev, range) in events {
        match ev {
            Event::Start(Tag::Link { .. }) => link_stack.push(range.start),
            Event::End(TagEnd::Link) => {
                let Some(open) = link_stack.pop() else { continue };
                if let Some((lo, hi)) = find_inline_dest_range(bytes, open, range.end) {
                    sites.push(InlineLinkDestinationSite { range: lo..hi });
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

fn wrappable_paragraphs(
    source: &str,
    events: &[(Event<'_>, Range<usize>)],
    autolinks: &[AutolinkFact],
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
                    && let Some(finished) = p.finish(bytes, autolinks)
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
                    && let Some(finished) = p.finish(bytes, autolinks)
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
                    && let Some(finished) = p.finish(bytes, autolinks)
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
    if bytes.get(start).copied()? != b'[' {
        return None;
    }
    let mut depth: i32 = 1;
    let mut i = start.saturating_add(1);
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

    fn finish(mut self, bytes: &[u8], extra_atomics: &[crate::AutolinkFact]) -> Option<WrappableParagraph> {
        let (line_lo, first_prefix) = extract_first_prefix(bytes, self.content_lo)?;
        let line_hi = extract_line_hi(bytes, self.content_hi);
        if is_mkdocs_admonition_paragraph(bytes, line_lo, line_hi) {
            return None;
        }
        let cont_prefix = derive_continuation_prefix(&first_prefix)?;
        for autolink in extra_atomics {
            let raw_range = autolink.raw_range();
            if raw_range.start >= self.content_lo && raw_range.end <= self.content_hi {
                self.atomics.push(raw_range);
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
            first_prefix,
            cont_prefix,
            atomics,
            hard_breaks,
        })
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
}
