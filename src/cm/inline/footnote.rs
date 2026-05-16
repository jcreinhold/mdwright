//! Footnote references (GFM extension; pairs with
//! [`crate::cm::block::footnote::FootnoteDef`]).
//!
//! Source form: `[^label]`. The IR carries the raw label; label
//! resolution against the document's collected definitions is the
//! formatter's responsibility at emission time.

use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootnoteReference<'a> {
    label: Cow<'a, str>,
}

impl<'a> FootnoteReference<'a> {
    #[tracing::instrument(level = "trace", skip(label))]
    pub(crate) fn new(label: Cow<'a, str>) -> Self {
        Self { label }
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_round_trips() {
        let r = FootnoteReference::new(Cow::Borrowed("foo"));
        assert_eq!(r.label(), "foo");
    }
}
