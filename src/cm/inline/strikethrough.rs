//! Strikethrough runs (GFM §6.5).
//!
//! Always emitted as `~~body~~`. The single-tilde variant is not
//! supported by GFM and pulldown-cmark never produces it. No
//! per-instance state to carry; the unit struct is the value-level
//! witness that the surrounding node is a strikethrough.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Strikethrough;

impl Strikethrough {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new() -> Self {
        Self
    }

    /// Wrap `body` in `~~…~~`.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'a>(body: crate::format::doc::Doc<'a>) -> crate::format::doc::Doc<'a> {
        use crate::format::doc::{concat, text};
        concat([text("~~"), body, text("~~")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strikethrough_is_uniquely_inhabited() {
        assert_eq!(Strikethrough::new(), Strikethrough);
    }
}
