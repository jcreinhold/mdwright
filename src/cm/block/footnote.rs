//! Footnote definitions (GFM extension; not in CM proper).
//!
//! `[^label]: text` introduces a footnote whose body lives as the
//! definition's children. The typed value carries the raw label as it
//! appeared in source; label normalisation (case folding, whitespace
//! collapse) is the formatter's job at emission time, not the IR's.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FootnoteDef {
    label: String,
}

impl FootnoteDef {
    #[tracing::instrument(level = "trace", skip(label))]
    pub(crate) fn new(label: String) -> Self {
        Self { label }
    }

    #[cfg(test)]
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_round_trips() {
        let d = FootnoteDef::new("foo".to_owned());
        assert_eq!(d.label(), "foo");
    }
}
