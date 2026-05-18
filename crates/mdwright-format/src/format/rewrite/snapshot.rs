use std::ops::Range;

use pulldown_cmark::{Event, Tag};

use crate::format::rewrite::candidate::{Candidate, Phase, Verification};
use mdwright_document::Document;
use mdwright_document::NormalisedLabel;
use mdwright_document::parse;
use mdwright_document::{CanonicalSource, Source};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OwnerId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerKind {
    Document,
    Paragraph,
    List,
    ListItem,
    DefinitionList,
    DefinitionDescription,
    FootnoteDefinition,
    ReferenceDefinition,
    Heading,
    MathRegion,
    Frontmatter,
    ThematicBreak,
}

#[derive(Clone, Debug)]
struct Owner {
    kind: OwnerKind,
    range: Range<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReferenceDestinationSite {
    pub(crate) owner: OwnerId,
    pub(crate) range: Range<usize>,
}

pub(crate) struct Snapshot<'a> {
    source: &'a str,
    document: Document,
    owners: Vec<Owner>,
    reference_destination_sites: Vec<ReferenceDestinationSite>,
}

impl<'a> Snapshot<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        let document = Document::parse(source);
        let mut snapshot = Self {
            source,
            document,
            owners: vec![Owner {
                kind: OwnerKind::Document,
                range: 0..source.len(),
            }],
            reference_destination_sites: Vec::new(),
        };
        snapshot.collect_event_owners();
        snapshot.collect_document_owners();
        snapshot.collect_reference_destination_sites();
        snapshot
    }

    pub(crate) fn source(&self) -> &'a str {
        self.source
    }

    pub(crate) fn document(&self) -> &Document {
        &self.document
    }

    pub(crate) fn reference_destination_sites(&self) -> &[ReferenceDestinationSite] {
        &self.reference_destination_sites
    }

    pub(crate) fn candidate(
        &self,
        phase: Phase,
        preferred_owner: OwnerKind,
        range: Range<usize>,
        replacement: String,
        verification: Verification,
        label: &'static str,
    ) -> Option<Candidate> {
        if !self.valid_range(&range) {
            return None;
        }
        let owner = self
            .find_owner(preferred_owner, &range)
            .or_else(|| self.smallest_owner_containing(&range))?;
        self.candidate_for_owner(owner, phase, range, replacement, verification, label)
    }

    pub(crate) fn candidate_for_owner(
        &self,
        owner: OwnerId,
        phase: Phase,
        range: Range<usize>,
        replacement: String,
        verification: Verification,
        label: &'static str,
    ) -> Option<Candidate> {
        if !self.valid_range(&range) || !self.owner_contains(owner, &range) {
            return None;
        }
        if !self.owner_allows(owner, verification, &range) {
            return None;
        }
        Some(Candidate::new(phase, owner, range, replacement, verification, label))
    }

    fn valid_range(&self, range: &Range<usize>) -> bool {
        range.start <= range.end
            && range.end <= self.source.len()
            && self.source.is_char_boundary(range.start)
            && self.source.is_char_boundary(range.end)
    }

    fn owner_contains(&self, owner: OwnerId, range: &Range<usize>) -> bool {
        let Some(owner) = self.owners.get(owner.0) else {
            return false;
        };
        owner.range.start <= range.start && owner.range.end >= range.end
    }

    fn owner_allows(&self, owner: OwnerId, verification: Verification, range: &Range<usize>) -> bool {
        let Some(owner) = self.owners.get(owner.0) else {
            return false;
        };
        match verification {
            Verification::PreserveMarkdownAndMath => true,
            Verification::MathRewrite => matches!(owner.kind, OwnerKind::MathRegion),
            Verification::RemoveFrontmatter => matches!(owner.kind, OwnerKind::Frontmatter) && owner.range == *range,
        }
    }

    fn push_owner(&mut self, kind: OwnerKind, range: Range<usize>) -> OwnerId {
        let id = OwnerId(self.owners.len());
        self.owners.push(Owner { kind, range });
        id
    }

    fn find_owner(&self, kind: OwnerKind, range: &Range<usize>) -> Option<OwnerId> {
        self.owners
            .iter()
            .enumerate()
            .filter(|(_, owner)| owner.kind == kind && owner.range.start <= range.start && owner.range.end >= range.end)
            .min_by_key(|(_, owner)| owner.range.end.saturating_sub(owner.range.start))
            .map(|(idx, _)| OwnerId(idx))
    }

    fn smallest_owner_containing(&self, range: &Range<usize>) -> Option<OwnerId> {
        self.owners
            .iter()
            .enumerate()
            .filter(|(_, owner)| owner.range.start <= range.start && owner.range.end >= range.end)
            .min_by_key(|(_, owner)| owner.range.end.saturating_sub(owner.range.start))
            .map(|(idx, _)| OwnerId(idx))
    }

    fn collect_event_owners(&mut self) {
        let src = Source::new(self.source);
        for (event, range) in parse::events_with_offsets(
            CanonicalSource::from_source(&src),
            parse::options(mdwright_document::ParseOptions::default()),
        ) {
            match event {
                Event::Start(Tag::Paragraph) => {
                    self.push_owner(OwnerKind::Paragraph, range);
                }
                Event::Start(Tag::Heading { .. }) => {
                    self.push_owner(OwnerKind::Heading, range);
                }
                Event::Start(Tag::List(_)) => {
                    self.push_owner(OwnerKind::List, range);
                }
                Event::Start(Tag::Item) => {
                    self.push_owner(OwnerKind::ListItem, range);
                }
                Event::Start(Tag::FootnoteDefinition(_)) => {
                    self.push_owner(OwnerKind::FootnoteDefinition, range);
                }
                Event::Start(Tag::DefinitionList) => {
                    self.push_owner(OwnerKind::DefinitionList, range);
                }
                Event::Start(Tag::DefinitionListDefinition) => {
                    self.push_owner(OwnerKind::DefinitionDescription, range);
                }
                Event::Rule => {
                    self.push_owner(OwnerKind::ThematicBreak, range);
                }
                Event::Start(_)
                | Event::End(_)
                | Event::Text(_)
                | Event::Code(_)
                | Event::InlineMath(_)
                | Event::DisplayMath(_)
                | Event::Html(_)
                | Event::InlineHtml(_)
                | Event::FootnoteReference(_)
                | Event::SoftBreak
                | Event::HardBreak
                | Event::TaskListMarker(_) => {}
            }
        }
    }

    fn collect_document_owners(&mut self) {
        let math_ranges: Vec<Range<usize>> = self
            .document
            .math_regions()
            .iter()
            .map(|region| region.range.clone())
            .collect();
        for range in math_ranges {
            self.push_owner(OwnerKind::MathRegion, range);
        }
        if let Some(frontmatter) = self.document.frontmatter() {
            let bytes = self.source.as_bytes();
            let mut end = frontmatter.slice.raw_range.end;
            while bytes.get(end).copied() == Some(b'\n') {
                end = end.saturating_add(1);
            }
            self.push_owner(OwnerKind::Frontmatter, frontmatter.slice.raw_range.start..end);
        }
    }

    fn collect_reference_destination_sites(&mut self) {
        let excluded = self.excluded_block_ranges();
        let mut seen = std::collections::HashSet::new();
        let bytes = self.source.as_bytes();
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
                let owner = self.push_owner(OwnerKind::ReferenceDefinition, line_start..line_end);
                self.reference_destination_sites.push(ReferenceDestinationSite {
                    owner,
                    range: site.dest,
                });
            }
            if line_end == bytes.len() {
                break;
            }
            line_start = line_end.saturating_add(1);
        }
    }

    fn excluded_block_ranges(&self) -> Vec<Range<usize>> {
        self.document
            .code_blocks()
            .iter()
            .map(|b| b.raw_range.clone())
            .chain(self.document.html_blocks().iter().map(|b| b.raw_range.clone()))
            .collect()
    }
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::format::rewrite::{Phase, Verification};

    use super::*;

    #[test]
    fn reference_definition_sites_skip_html_block_contents() {
        let snapshot = Snapshot::new("<?J\n\n[_]:#");
        assert!(snapshot.reference_destination_sites().is_empty());
    }

    #[test]
    fn candidate_requires_owner_to_cover_range() {
        let snapshot = Snapshot::new("# h\n\nx\n");
        let owner = snapshot.find_owner(OwnerKind::Heading, &(0..3)).expect("heading owner");
        assert!(
            snapshot
                .candidate_for_owner(
                    owner,
                    Phase::HeadingAttrs,
                    0..6,
                    "# h\n\n".to_owned(),
                    Verification::PreserveMarkdownAndMath,
                    "heading",
                )
                .is_none()
        );
    }
}
