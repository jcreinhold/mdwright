use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Phase {
    Italic,
    Strong,
    UnorderedList,
    OrderedList,
    ThematicBreak,
    Table,
    LinkDestination,
    HeadingAttrs,
    Math,
    Frontmatter,
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verification {
    PreserveMarkdownAndMath,
    MathRewrite,
    RemoveFrontmatter,
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    phase: Phase,
    owner: super::snapshot::OwnerId,
    range: Range<usize>,
    replacement: String,
    verification: Verification,
    label: &'static str,
}

impl Candidate {
    pub(super) fn new(
        phase: Phase,
        owner: super::snapshot::OwnerId,
        range: Range<usize>,
        replacement: String,
        verification: Verification,
        label: &'static str,
    ) -> Self {
        Self {
            phase,
            owner,
            range,
            replacement,
            verification,
            label,
        }
    }

    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }

    pub(crate) fn owner(&self) -> super::snapshot::OwnerId {
        self.owner
    }

    pub(crate) fn range(&self) -> &Range<usize> {
        &self.range
    }

    pub(crate) fn replacement(&self) -> &str {
        &self.replacement
    }

    pub(crate) fn verification(&self) -> Verification {
        self.verification
    }

    pub(crate) fn label(&self) -> &'static str {
        self.label
    }
}
