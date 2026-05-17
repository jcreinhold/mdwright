//! Footnote references (GFM extension; pairs with
//! [`crate::cm::block::footnote::FootnoteDef`]).
//!
//! Source form: `[^label]`. The IR carries the raw label; label
//! resolution against the document's collected definitions is the
//! formatter's responsibility at emission time.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootnoteReference {
    label: String,
}

impl FootnoteReference {
    #[tracing::instrument(level = "trace", skip(label))]
    pub(crate) fn new(label: String) -> Self {
        Self { label }
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    /// Emit `[^label]` as a plain text doc.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self) -> crate::format::doc::Doc<'b> {
        use crate::format::doc::text;
        text(format!("[^{}]", self.label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_round_trips() {
        let r = FootnoteReference::new("foo".to_owned());
        assert_eq!(r.label(), "foo");
    }
}
