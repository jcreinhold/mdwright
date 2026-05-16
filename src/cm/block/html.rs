//! HTML blocks (CM §4.6).
//!
//! Pulldown-cmark has already classified the surrounding bytes as one
//! of the seven HTML-block opener conditions and accumulated the body
//! verbatim. The typed value carries the body so prompt 27's emitter
//! can write it without re-walking the tree. The bytes are
//! prefix-stripped (no leading container indent / blockquote markers)
//! — the surrounding block emitter re-applies those when needed.

use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HtmlBlock<'a> {
    body: Cow<'a, str>,
}

impl<'a> HtmlBlock<'a> {
    #[tracing::instrument(level = "trace", skip(body))]
    pub(crate) fn new(body: Cow<'a, str>) -> Self {
        Self { body }
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_round_trips() {
        let b = HtmlBlock::new(Cow::Borrowed("<div>x</div>\n"));
        assert_eq!(b.body(), "<div>x</div>\n");
    }
}
