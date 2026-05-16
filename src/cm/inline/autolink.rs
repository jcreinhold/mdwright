//! Typed `CommonMark` autolinks.
//!
//! [`AutolinkRun`] carries the raw URL pulldown extracted from a
//! `<…>` autolink, together with a kind discriminant (URI vs email).
//! Both `from_cmark_*` constructors are infallible: pulldown already
//! validated the autolink grammar at parse time, so existence of an
//! `AutolinkRun` value is evidence that the bytes round-trip through
//! the CM tokenizer.
//!
//! GFM extended autolinks (bare URLs in text) are deliberately not
//! handled here. They reach mdwright only through the `bare-url`
//! linter (`src/stdlib/bare_url.rs`), never through the IR, so adding
//! a constructor for them this session would ship surface area with no
//! caller. When the linter or a future autofix wants a typed value, a
//! fallible `try_from_gfm_extended` constructor lands alongside the
//! existing regex without changing the rest of the surface.

use std::borrow::Cow;

/// Classification of an [`AutolinkRun`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum AutolinkKind {
    /// `<https://example.com>` (CM §6.5).
    Uri,
    /// `<user@example.com>` (CM §6.6).
    Email,
}

/// Typed CM autolink.
#[derive(Clone, Debug)]
pub struct AutolinkRun<'a> {
    url: Cow<'a, str>,
    #[cfg_attr(not(test), allow(dead_code))]
    kind: AutolinkKind,
}

impl<'a> AutolinkRun<'a> {
    pub(crate) fn from_cmark_uri(url: Cow<'a, str>) -> Self {
        Self {
            url,
            kind: AutolinkKind::Uri,
        }
    }

    pub(crate) fn from_cmark_email(url: Cow<'a, str>) -> Self {
        Self {
            url,
            kind: AutolinkKind::Email,
        }
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Emit `<url>` as a single unbreakable token.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self) -> crate::format::doc::Doc<'b> {
        use crate::format::doc::{text, unbreakable};
        unbreakable(text(format!("<{}>", self.url)))
    }

    #[cfg(test)]
    pub(crate) fn kind(&self) -> AutolinkKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_constructor_preserves_url() {
        let run = AutolinkRun::from_cmark_uri(Cow::Borrowed("https://example.com"));
        assert_eq!(run.url(), "https://example.com");
        assert_eq!(run.kind(), AutolinkKind::Uri);
    }

    #[test]
    fn email_constructor_preserves_url() {
        let run = AutolinkRun::from_cmark_email(Cow::Borrowed("user@example.com"));
        assert_eq!(run.url(), "user@example.com");
        assert_eq!(run.kind(), AutolinkKind::Email);
    }
}
