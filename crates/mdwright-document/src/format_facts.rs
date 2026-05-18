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

use crate::heading::find_attr_trailer_range;
use crate::ir::BlockCheckpointFact;
use crate::refs::NormalisedLabel;
use crate::source::{ByteSpan, CanonicalSource, Source};
use crate::{Document, HeadingAttrs, ParseOptions, parse};

/// Structural owner kinds with source ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralKind {
    Paragraph,
    Heading,
    List,
    ListItem,
    DefinitionList,
    DefinitionDescription,
    FootnoteDefinition,
    ThematicBreak,
}

/// A recognised block/container range.
#[derive(Clone, Debug)]
pub struct StructuralSpan {
    pub kind: StructuralKind,
    pub raw_range: Range<usize>,
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
    pub open_lo: usize,
    pub open_hi: usize,
    pub close_lo: usize,
    pub close_hi: usize,
}

/// One unordered list and the byte offsets of its item markers.
#[derive(Clone, Debug)]
pub struct UnorderedListSite {
    pub raw_range: Range<usize>,
    pub bullets: Vec<usize>,
}

/// One ordered list and the digit ranges of its item markers.
#[derive(Clone, Debug)]
pub struct OrderedListSite {
    pub raw_range: Range<usize>,
    pub items: Vec<OrderedItemSite>,
}

#[derive(Clone, Debug)]
pub struct OrderedItemSite {
    pub marker_lo: usize,
    pub marker_hi: usize,
}

/// An ATX heading attribute trailer.
#[derive(Clone, Debug)]
pub struct HeadingAttrSite {
    pub attrs: HeadingAttrs,
    pub trailer: Range<usize>,
}

/// An inline link destination byte range.
#[derive(Clone, Debug)]
pub struct InlineLinkDestinationSite {
    pub range: Range<usize>,
}

/// A link-reference definition destination byte range.
#[derive(Clone, Debug)]
pub struct ReferenceDefinitionSite {
    pub raw_range: Range<usize>,
    pub destination: Range<usize>,
}

/// A paragraph range with the inline facts needed by the wrap pass.
#[derive(Clone, Debug)]
pub struct WrappableParagraph {
    pub line_lo: usize,
    pub line_hi: usize,
    pub content_lo: usize,
    pub content_hi: usize,
    pub first_prefix: String,
    pub cont_prefix: String,
    pub atomics: Vec<Range<usize>>,
    pub hard_breaks: Vec<ParagraphHardBreak>,
}

#[derive(Clone, Debug)]
pub struct ParagraphHardBreak {
    pub marker_lo: usize,
    pub nl: usize,
    pub marker: &'static str,
}

impl Document {
    /// Recognised block/container ranges used as rewrite owners.
    #[must_use]
    pub fn structural_spans(&self) -> Vec<StructuralSpan> {
        let mut out = Vec::new();
        for (event, range) in self.events_with_offsets() {
            match event {
                Event::Start(Tag::Paragraph) => out.push(StructuralSpan {
                    kind: StructuralKind::Paragraph,
                    raw_range: range,
                }),
                Event::Start(Tag::Heading { .. }) => out.push(StructuralSpan {
                    kind: StructuralKind::Heading,
                    raw_range: range,
                }),
                Event::Start(Tag::List(_)) => out.push(StructuralSpan {
                    kind: StructuralKind::List,
                    raw_range: range,
                }),
                Event::Start(Tag::Item) => out.push(StructuralSpan {
                    kind: StructuralKind::ListItem,
                    raw_range: range,
                }),
                Event::Start(Tag::FootnoteDefinition(_)) => out.push(StructuralSpan {
                    kind: StructuralKind::FootnoteDefinition,
                    raw_range: range,
                }),
                Event::Start(Tag::DefinitionList) => out.push(StructuralSpan {
                    kind: StructuralKind::DefinitionList,
                    raw_range: range,
                }),
                Event::Start(Tag::DefinitionListDefinition) => out.push(StructuralSpan {
                    kind: StructuralKind::DefinitionDescription,
                    raw_range: range,
                }),
                Event::Rule => out.push(StructuralSpan {
                    kind: StructuralKind::ThematicBreak,
                    raw_range: range,
                }),
                _ => {}
            }
        }
        out
    }

    /// Inline emphasis/strong delimiter ranges.
    #[must_use]
    pub fn inline_delimiter_spans(&self, kind: InlineDelimiterKind) -> Vec<InlineDelimiterSpan> {
        let mut starts: Vec<usize> = Vec::new();
        let mut spans: Vec<InlineDelimiterSpan> = Vec::new();
        let delim_len = match kind {
            InlineDelimiterKind::Emphasis => 1,
            InlineDelimiterKind::Strong => 2,
        };
        let bytes = self.source().as_bytes();
        for (ev, range) in self.events_with_offsets() {
            if delimiter_matches_start(&ev, kind) {
                starts.push(range.start);
            } else if delimiter_matches_end(&ev, kind) {
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

    /// Unordered list marker sites.
    #[must_use]
    pub fn unordered_list_sites(&self) -> Vec<UnorderedListSite> {
        let bytes = self.source().as_bytes();
        let mut stack: Vec<(bool, UnorderedListSite)> = Vec::new();
        let mut completed = Vec::new();
        for (ev, range) in self.events_with_offsets() {
            match ev {
                Event::Start(Tag::List(start)) => {
                    stack.push((
                        start.is_none(),
                        UnorderedListSite {
                            raw_range: range,
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

    /// Ordered list marker digit sites.
    #[must_use]
    pub fn ordered_list_sites(&self) -> Vec<OrderedListSite> {
        let bytes = self.source().as_bytes();
        let mut stack: Vec<(bool, OrderedListSite)> = Vec::new();
        let mut completed = Vec::new();
        for (ev, range) in self.events_with_offsets() {
            match ev {
                Event::Start(Tag::List(start)) => {
                    stack.push((
                        start.is_some(),
                        OrderedListSite {
                            raw_range: range,
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

    /// Thematic break source line ranges.
    #[must_use]
    pub fn thematic_break_ranges(&self) -> Vec<Range<usize>> {
        let mut sites = Vec::new();
        let bytes = self.source().as_bytes();
        for (ev, range) in self.events_with_offsets() {
            if matches!(ev, Event::Rule) {
                let mut hi = range.end.min(bytes.len());
                while hi > range.start && matches!(bytes.get(hi.saturating_sub(1)).copied(), Some(b'\n' | b'\r')) {
                    hi = hi.saturating_sub(1);
                }
                sites.push(range.start..hi);
            }
        }
        sites
    }

    /// Heading attribute trailer sites.
    #[must_use]
    pub fn heading_attr_sites(&self) -> Vec<HeadingAttrSite> {
        let mut sites = Vec::new();
        for (ev, range) in self.events_with_offsets() {
            if let Event::Start(Tag::Heading { id, classes, attrs, .. }) = ev
                && (id.is_some() || !classes.is_empty() || !attrs.is_empty())
            {
                let heading_attrs = HeadingAttrs {
                    id: id.map(|s| s.to_string()),
                    classes: classes.iter().map(|c| c.to_string()).collect(),
                    attrs: attrs
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.as_ref().map(std::string::ToString::to_string)))
                        .collect(),
                    source_trailer: String::new(),
                };
                let Some(slice) = self.source().get(range.clone()) else {
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

    /// Inline link destination ranges.
    #[must_use]
    pub fn inline_link_destination_sites(&self) -> Vec<InlineLinkDestinationSite> {
        let bytes = self.source().as_bytes();
        let mut sites = Vec::new();
        let mut link_stack = Vec::new();
        for (ev, range) in self.events_with_offsets() {
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

    /// Reference-definition destination ranges.
    #[must_use]
    pub fn reference_definition_sites(&self) -> Vec<ReferenceDefinitionSite> {
        let excluded = self.excluded_block_ranges();
        let mut seen = std::collections::HashSet::new();
        let bytes = self.source().as_bytes();
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

    /// Paragraph ranges and inline atomics for the wrap pass.
    #[must_use]
    pub fn wrappable_paragraphs(&self) -> Vec<WrappableParagraph> {
        let mut paragraphs = Vec::new();
        let bytes = self.source().as_bytes();
        let mut current: Option<PartialParagraph> = None;
        let mut paragraph_depth: u32 = 0;
        let mut prose_container_depth: u32 = 0;

        for (ev, range) in self.events_with_offsets() {
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
                        && let Some(finished) = p.finish(bytes)
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
                        && let Some(finished) = p.finish(bytes)
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
                        && let Some(finished) = p.finish(bytes)
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

    fn events_with_offsets(&self) -> Vec<(Event<'_>, Range<usize>)> {
        parse::collect_events_with_offsets(
            CanonicalSource::from_source(self.source_handle()),
            parse::options(self.parse_options()),
        )
        .unwrap_or_default()
    }

    fn excluded_block_ranges(&self) -> Vec<Range<usize>> {
        self.code_blocks()
            .iter()
            .map(|b| b.raw_range.clone())
            .chain(self.html_blocks().iter().map(|b| b.raw_range.clone()))
            .collect()
    }
}

/// Top-level block checkpoints in original source coordinates.
///
/// # Errors
///
/// Returns [`crate::ParseError`] if parser execution cannot safely
/// recognise the canonicalised source.
pub fn top_level_block_checkpoints(
    source: &str,
    opts: ParseOptions,
) -> Result<Vec<BlockCheckpointFact>, crate::ParseError> {
    let source_len = u32::try_from(source.len()).unwrap_or(u32::MAX);
    let src = Source::new(source);
    let canonical = src.canonical();
    let map_is_identity = src.offset_map().is_identity();
    let fm_end = frontmatter_end(canonical);
    let body = CanonicalSource::from_source(&src).trusted_subrange(fm_end..canonical.len());
    let cap = (source.len() / 64).saturating_add(2);
    let mut points = Vec::with_capacity(cap);
    points.push(BlockCheckpointFact {
        byte: 0,
        parser_state: 0,
    });

    let mut depth: u32 = 0;
    let mut event_count: u32 = 0;
    let try_push = |points: &mut Vec<BlockCheckpointFact>, range_start: usize, depth: u32, event_count: u32| {
        let abs_canonical = u32::try_from(range_start.saturating_add(fm_end)).unwrap_or(u32::MAX);
        let abs_original = if map_is_identity {
            abs_canonical
        } else {
            src.to_original(ByteSpan::new(abs_canonical, abs_canonical)).start
        };
        if points.last().is_none_or(|last| last.byte < abs_original) {
            points.push(BlockCheckpointFact {
                byte: abs_original,
                parser_state: parser_state_hash(depth, event_count),
            });
        }
    };
    for (event, range) in parse::collect_events_with_offsets(body, parse::options(opts))? {
        event_count = event_count.saturating_add(1);
        walk_checkpoint_event(event, range.start, &mut depth, event_count, &mut points, &try_push);
    }
    if points.last().is_none_or(|last| last.byte < source_len) {
        points.push(BlockCheckpointFact {
            byte: source_len,
            parser_state: parser_state_hash(depth, event_count),
        });
    }
    Ok(points)
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

    fn finish(self, bytes: &[u8]) -> Option<WrappableParagraph> {
        let (line_lo, first_prefix) = extract_first_prefix(bytes, self.content_lo)?;
        let line_hi = extract_line_hi(bytes, self.content_hi);
        let cont_prefix = derive_continuation_prefix(&first_prefix)?;
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

fn frontmatter_end(source: &str) -> usize {
    let Some(first_line_end) = source.find('\n') else {
        return 0;
    };
    let first_line = source.get(..first_line_end).unwrap_or("");
    let trimmed = first_line.trim_end();
    let close_pat: &[&str] = match trimmed {
        "---" => &["---", "..."],
        "+++" => &["+++"],
        _ => return 0,
    };
    let body_start = first_line_end.saturating_add(1);
    let Some(rest) = source.get(body_start..) else {
        return 0;
    };
    let mut cursor = 0usize;
    let mut saw_key = false;
    while cursor < rest.len() {
        let nl = rest
            .get(cursor..)
            .and_then(|s| s.find('\n'))
            .unwrap_or_else(|| rest.len().saturating_sub(cursor));
        let end_excl = cursor.saturating_add(nl);
        let line = rest.get(cursor..end_excl).unwrap_or("");
        let line_trim = line.trim_end();
        if close_pat.contains(&line_trim) {
            if !saw_key {
                return 0;
            }
            return body_start.saturating_add(end_excl).saturating_add(1).min(source.len());
        }
        if line_has_key(line_trim, trimmed == "+++") {
            saw_key = true;
        }
        cursor = end_excl.saturating_add(1);
    }
    0
}

fn line_has_key(line: &str, toml: bool) -> bool {
    let trimmed = line.trim_start();
    let sep = if toml { '=' } else { ':' };
    let Some(idx) = trimmed.find(sep) else {
        return false;
    };
    let key = trimmed.get(..idx).unwrap_or("").trim();
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn walk_checkpoint_event(
    event: Event<'_>,
    range_start: usize,
    depth: &mut u32,
    event_count: u32,
    points: &mut Vec<BlockCheckpointFact>,
    try_push: &impl Fn(&mut Vec<BlockCheckpointFact>, usize, u32, u32),
) {
    match event {
        Event::Start(tag) if *depth == 0 && is_top_level_block(&tag) => {
            try_push(points, range_start, *depth, event_count);
            if is_container(&tag) {
                *depth = depth.saturating_add(1);
            }
        }
        Event::Start(tag) if is_container(&tag) => {
            *depth = depth.saturating_add(1);
        }
        Event::End(end) if is_container_end(end) => {
            *depth = depth.saturating_sub(1);
        }
        Event::Rule if *depth == 0 => {
            try_push(points, range_start, *depth, event_count);
        }
        _ => {}
    }
}

fn is_top_level_block(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Table(_)
            | Tag::FootnoteDefinition(_)
    )
}

fn is_container(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::BlockQuote(_)
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
    )
}

fn is_container_end(end: TagEnd) -> bool {
    matches!(
        end,
        TagEnd::BlockQuote(_)
            | TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::FootnoteDefinition
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::TableCell
    )
}

fn parser_state_hash(depth: u32, event_count: u32) -> u64 {
    (u64::from(depth) << 32) | u64::from(event_count)
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
            .into_iter()
            .next()
            .expect("footnote definition paragraph");
        assert_eq!(paragraph.cont_prefix, "    ");
    }

    #[test]
    fn definition_list_continuation_uses_marker_width_indent() {
        let doc = Document::parse("term\n:   alpha beta gamma\n").expect("fixture parses");
        let paragraph = doc
            .wrappable_paragraphs()
            .into_iter()
            .next()
            .expect("definition list paragraph");
        assert_eq!(paragraph.cont_prefix, "    ");
    }
}
