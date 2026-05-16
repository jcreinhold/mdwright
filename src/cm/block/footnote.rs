//! Footnote definitions (GFM extension; not in CM proper).
//!
//! `[^label]: text` introduces a footnote whose body lives as the
//! definition's children. The typed value carries the raw label as it
//! appeared in source; label normalisation (case folding, whitespace
//! collapse) is the formatter's job at emission time, not the IR's.

use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FootnoteDef<'a> {
    label: Cow<'a, str>,
}

impl<'a> FootnoteDef<'a> {
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
        let d = FootnoteDef::new(Cow::Borrowed("foo"));
        assert_eq!(d.label(), "foo");
    }
}
