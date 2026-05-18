//! Definition lists (mdformat-mkdocs / python-markdown extension,
//! pulldown `Tag::DefinitionList` under `Options::ENABLE_DEFINITION_LIST`).
//!
//! The typed value is a unit struct: the IR records that a
//! `Tag::DefinitionList` was seen so lint rules can act on it; the
//! structural emit preserves the source bytes as-is. The canonicalise
//! pass owns any deliberate reshaping if a future style knob calls
//! for it.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct DefinitionList;

impl DefinitionList {
    pub(crate) fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_list_is_uniquely_inhabited() {
        assert_eq!(DefinitionList::new(), DefinitionList);
    }
}
