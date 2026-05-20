use std::ops::Range;

use crate::format::rewrite::candidate::{Candidate, Verification};
use mdwright_document::{Document, ParseError, ParseOptions, StructuralKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OwnerId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerKind {
    Document,
    Paragraph,
    BlockQuote,
    List,
    ListItem,
    DefinitionList,
    DefinitionDescription,
    FootnoteDefinition,
    InlineDelimiterSlot,
    InlineLinkDestination,
    ReferenceDefinition,
    Table,
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
    document: SnapshotDocument<'a>,
    owners: Vec<Owner>,
    reference_destination_sites: Vec<ReferenceDestinationSite>,
}

enum SnapshotDocument<'a> {
    Borrowed(&'a Document),
    Owned(Box<Document>),
}

impl<'a> Snapshot<'a> {
    pub(crate) fn from_document(document: &'a Document) -> Self {
        let source = document.source();
        let mut snapshot = Self {
            source,
            document: SnapshotDocument::Borrowed(document),
            owners: vec![Owner {
                kind: OwnerKind::Document,
                range: 0..source.len(),
            }],
            reference_destination_sites: Vec::new(),
        };
        snapshot.collect_event_owners();
        snapshot.collect_document_owners();
        snapshot.collect_inline_slot_owners();
        snapshot.collect_reference_destination_sites();
        snapshot
    }

    pub(crate) fn parse_owned(source: &'a str, parse_options: ParseOptions) -> Result<Self, ParseError> {
        let document = Document::parse_with_options(source, parse_options)?;
        let mut snapshot = Self {
            source,
            document: SnapshotDocument::Owned(Box::new(document)),
            owners: vec![Owner {
                kind: OwnerKind::Document,
                range: 0..source.len(),
            }],
            reference_destination_sites: Vec::new(),
        };
        snapshot.collect_event_owners();
        snapshot.collect_document_owners();
        snapshot.collect_inline_slot_owners();
        snapshot.collect_reference_destination_sites();
        Ok(snapshot)
    }

    pub(crate) fn source(&self) -> &'a str {
        self.source
    }

    pub(crate) fn document(&self) -> &Document {
        match &self.document {
            SnapshotDocument::Borrowed(document) => document,
            SnapshotDocument::Owned(document) => document,
        }
    }

    pub(crate) fn reference_destination_sites(&self) -> &[ReferenceDestinationSite] {
        &self.reference_destination_sites
    }

    pub(crate) fn candidate(
        &self,
        owner_kind: OwnerKind,
        range: Range<usize>,
        replacement: String,
        verification: Verification,
        label: &'static str,
    ) -> Option<Candidate> {
        if !self.valid_range(&range) {
            return None;
        }
        let owner = self.find_owner(owner_kind, &range)?;
        self.candidate_for_owner(owner, range, replacement, verification, label)
    }

    pub(crate) fn candidate_for_owner(
        &self,
        owner: OwnerId,
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
        Some(Candidate::new(owner, range, replacement, verification, label))
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

    fn collect_event_owners(&mut self) {
        let spans: Vec<_> = self.document().structural_spans().to_vec();
        for span in spans {
            self.push_owner(owner_kind_from_structural(span.kind()), span.raw_range());
        }
    }

    fn collect_document_owners(&mut self) {
        let math_ranges: Vec<Range<usize>> = self
            .document()
            .math_regions()
            .iter()
            .map(|region| region.range.clone())
            .collect();
        for range in math_ranges {
            self.push_owner(OwnerKind::MathRegion, range);
        }
        if let Some(frontmatter) = self.document().frontmatter() {
            let bytes = self.source.as_bytes();
            let mut end = frontmatter.slice.raw_range.end;
            while bytes.get(end).copied() == Some(b'\n') {
                end = end.saturating_add(1);
            }
            self.push_owner(OwnerKind::Frontmatter, frontmatter.slice.raw_range.start..end);
        }
    }

    fn collect_inline_slot_owners(&mut self) {
        let delimiter_ranges: Vec<Range<usize>> = self
            .document()
            .inline_delimiter_slots(mdwright_document::InlineDelimiterKind::Emphasis)
            .iter()
            .chain(
                self.document()
                    .inline_delimiter_slots(mdwright_document::InlineDelimiterKind::Strong)
                    .iter(),
            )
            .flat_map(|slot| [slot.open_range(), slot.close_range()])
            .collect();
        for range in delimiter_ranges {
            self.push_owner(OwnerKind::InlineDelimiterSlot, range);
        }

        let link_ranges: Vec<Range<usize>> = self
            .document()
            .inline_link_destination_slots()
            .iter()
            .map(mdwright_document::InlineLinkDestinationSlot::range)
            .collect();
        for range in link_ranges {
            self.push_owner(OwnerKind::InlineLinkDestination, range);
        }
    }

    fn collect_reference_destination_sites(&mut self) {
        let sites: Vec<_> = self.document().reference_definition_sites().to_vec();
        for site in sites {
            let owner = self.push_owner(OwnerKind::ReferenceDefinition, site.raw_range());
            self.reference_destination_sites.push(ReferenceDestinationSite {
                owner,
                range: site.destination(),
            });
        }
    }
}

fn owner_kind_from_structural(kind: StructuralKind) -> OwnerKind {
    match kind {
        StructuralKind::Paragraph => OwnerKind::Paragraph,
        StructuralKind::Heading => OwnerKind::Heading,
        StructuralKind::BlockQuote => OwnerKind::BlockQuote,
        StructuralKind::List => OwnerKind::List,
        StructuralKind::ListItem => OwnerKind::ListItem,
        StructuralKind::DefinitionList => OwnerKind::DefinitionList,
        StructuralKind::DefinitionDescription => OwnerKind::DefinitionDescription,
        StructuralKind::FootnoteDefinition => OwnerKind::FootnoteDefinition,
        StructuralKind::ThematicBreak => OwnerKind::ThematicBreak,
        StructuralKind::Table => OwnerKind::Table,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::format::rewrite::Verification;

    use super::*;

    #[test]
    fn reference_definition_sites_skip_html_block_contents() {
        let snapshot = Snapshot::parse_owned("<?J\n\n[_]:#", ParseOptions::default()).expect("snapshot parses");
        assert!(snapshot.reference_destination_sites().is_empty());
    }

    #[test]
    fn candidate_requires_owner_to_cover_range() {
        let snapshot = Snapshot::parse_owned("# h\n\nx\n", ParseOptions::default()).expect("snapshot parses");
        let owner = snapshot.find_owner(OwnerKind::Heading, &(0..3)).expect("heading owner");
        assert!(
            snapshot
                .candidate_for_owner(
                    owner,
                    0..6,
                    "# h\n\n".to_owned(),
                    Verification::PreserveMarkdownAndMath,
                    "heading",
                )
                .is_none()
        );
    }

    #[test]
    fn candidate_requires_requested_owner_kind() {
        let snapshot =
            Snapshot::parse_owned("[x](https://example.com)\n", ParseOptions::default()).expect("snapshot parses");
        assert!(
            snapshot
                .candidate(
                    OwnerKind::ReferenceDefinition,
                    4..23,
                    "<https://example.com>".to_owned(),
                    Verification::PreserveMarkdownAndMath,
                    "inline-link",
                )
                .is_none()
        );
        assert!(
            snapshot
                .candidate(
                    OwnerKind::InlineLinkDestination,
                    4..23,
                    "<https://example.com>".to_owned(),
                    Verification::PreserveMarkdownAndMath,
                    "inline-link",
                )
                .is_some()
        );
    }

    #[test]
    fn inline_delimiter_slot_owner_covers_only_the_slot() {
        let snapshot = Snapshot::parse_owned("_x_\n", ParseOptions::default()).expect("snapshot parses");
        assert!(
            snapshot
                .candidate(
                    OwnerKind::InlineDelimiterSlot,
                    0..1,
                    "*".to_owned(),
                    Verification::PreserveMarkdownAndMath,
                    "italic",
                )
                .is_some()
        );
        assert!(
            snapshot
                .candidate(
                    OwnerKind::InlineDelimiterSlot,
                    0..3,
                    "*x*".to_owned(),
                    Verification::PreserveMarkdownAndMath,
                    "italic",
                )
                .is_none()
        );
    }
}
