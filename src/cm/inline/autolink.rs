//! Typed `CommonMark` autolinks.
//!
//! [`AutolinkRun`] carries the raw URL pulldown extracted from a
//! `<…>` autolink. Pulldown distinguishes URI and email autolinks at
//! parse time but mdwright emits both identically as `<url>`, so the
//! discriminant is dropped at construction — only the raw URL bytes
//! survive into the IR.
//!
//! GFM extended autolinks (bare URLs in text) are deliberately not
//! handled here. They reach mdwright only through the `bare-url`
//! linter (`src/stdlib/bare_url.rs`), never through the IR, so adding
//! a constructor for them this session would ship surface area with no
//! caller.

/// Typed CM autolink.
#[derive(Clone, Debug)]
pub struct AutolinkRun {
    url: String,
}

impl AutolinkRun {
    pub(crate) fn new(url: String) -> Self {
        Self { url }
    }

    #[cfg(test)]
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Emit `<url>` as a single unbreakable token.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self) -> crate::format::doc::Doc<'b> {
        use crate::format::doc::{text, unbreakable};
        unbreakable(text(format!("<{}>", self.url)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_preserves_url() {
        let run = AutolinkRun::new("https://example.com".to_owned());
        assert_eq!(run.url(), "https://example.com");
    }
}
