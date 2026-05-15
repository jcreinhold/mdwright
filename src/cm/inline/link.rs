//! Typed link / image values.
//!
//! [`LinkRun`] and [`ImageRun`] capture the parse-time data pulldown saw
//! and expose a single decision method, [`LinkRun::emit_style`] /
//! [`ImageRun::emit_style`], that selects the final emission style from
//! four previously-braided concerns: the CM grammar variant (`Inline` /
//! `ReferenceFull` / `Collapsed` / `Shortcut`), the configured emission style,
//! the CM label-text identity for collapsed/shortcut forms, and the
//! post-format text drift that emphasis rewriting can introduce.
//!
//! The IR builder ([`crate::tree::TreeBuilder`]) constructs values via
//! the infallible `from_pulldown_*` constructors. The format walker
//! ([`crate::format::inline`]) renders the body, flattens it via
//! [`flatten_body_doc`], and calls `emit_style` once — the sole site
//! that decides the final style.
//!
//! [`ReferenceHandle`] and [`ReferenceTable`] are skeletons this
//! session; Phase R prompt 23 will replace them with a real two-pass
//! reference resolver. Until then, `ReferenceHandle` carries only the
//! label string pulldown extracted, and `emit_style` does not consult
//! any external table — the body-vs-label identity check happens on
//! the label carried by the handle.

use std::borrow::Cow;

use crate::format::doc::Doc;

/// Source CM grammar variant pulldown classified the link as.
///
/// `Inline` is reached only through `from_pulldown_inline`; the three
/// reference variants are reached through `from_pulldown_reference`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkSourceKind {
    #[cfg_attr(not(test), allow(dead_code))]
    Inline,
    ReferenceFull,
    ReferenceCollapsed,
    ReferenceShortcut,
}

/// Source-side data for the link or image, mirroring the four CM
/// grammar variants. Inline carries no handle; the three reference
/// forms each carry the label they were associated with at parse time.
#[derive(Clone, Debug)]
pub(crate) enum LinkSource<'a> {
    Inline,
    ReferenceFull(ReferenceHandle<'a>),
    ReferenceCollapsed(ReferenceHandle<'a>),
    ReferenceShortcut(ReferenceHandle<'a>),
}

#[cfg(test)]
impl<'a> LinkSource<'a> {
    pub(crate) fn kind(&self) -> LinkSourceKind {
        match self {
            Self::Inline => LinkSourceKind::Inline,
            Self::ReferenceFull(_) => LinkSourceKind::ReferenceFull,
            Self::ReferenceCollapsed(_) => LinkSourceKind::ReferenceCollapsed,
            Self::ReferenceShortcut(_) => LinkSourceKind::ReferenceShortcut,
        }
    }

    fn handle(&self) -> Option<&ReferenceHandle<'a>> {
        match self {
            Self::Inline => None,
            Self::ReferenceFull(h) | Self::ReferenceCollapsed(h) | Self::ReferenceShortcut(h) => {
                Some(h)
            }
        }
    }
}

/// Opaque pointer to a link reference definition. In prompt 22 this
/// carries only the label string; prompt 23 will replace the internals
/// with an index into a resolved [`ReferenceTable`] without changing
/// the public surface.
#[derive(Clone, Debug)]
pub(crate) struct ReferenceHandle<'a> {
    label: Cow<'a, str>,
}

impl<'a> ReferenceHandle<'a> {
    pub(crate) fn new(label: Cow<'a, str>) -> Self {
        Self { label }
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

/// Skeleton reference table. Phase R prompt 23 will fill this in with
/// a real two-pass resolver; the placeholder lives here so the
/// public surface of [`LinkRun`] / [`ImageRun`] stays stable across
/// the prompt boundary.
#[allow(dead_code)]
#[derive(Default)]
pub(crate) struct ReferenceTable {
    _marker: (),
}

#[allow(dead_code)]
impl ReferenceTable {
    pub(crate) fn empty() -> Self {
        Self::default()
    }
}

/// Typed inline link.
#[derive(Clone, Debug)]
pub struct LinkRun<'a> {
    dest: Cow<'a, str>,
    title: Cow<'a, str>,
    source: LinkSource<'a>,
}

/// Typed inline image. `dest` is the image URL pulldown extracted at
/// parse time; the format walker re-emits it via the same URL-escape
/// path as [`LinkRun`].
#[derive(Clone, Debug)]
pub struct ImageRun<'a> {
    dest: Cow<'a, str>,
    title: Cow<'a, str>,
    source: LinkSource<'a>,
}

/// Format-time context for [`LinkRun::emit_style`] /
/// [`ImageRun::emit_style`]. Carries the actually-rendered body text
/// the walker has computed by emitting child nodes; this is the only
/// text the identity check sees, so post-emphasis-resolution drift
/// (e.g. `_foo_` → `*foo*`) is folded in automatically.
pub(crate) struct LinkResolveCtx<'a> {
    pub body_text: &'a str,
}

/// Final emission style chosen by `emit_style`. Carries the label
/// borrow when the chosen variant needs one, so the walker does not
/// reach back into the source to retrieve it.
#[derive(Debug)]
pub(crate) enum EmitLinkStyle<'a> {
    Inline,
    ReferenceFull { label: &'a str },
    ReferenceCollapsed,
    ReferenceShortcut,
}

impl<'a> LinkRun<'a> {
    pub(crate) fn from_pulldown_inline(dest: Cow<'a, str>, title: Cow<'a, str>) -> Self {
        Self {
            dest,
            title,
            source: LinkSource::Inline,
        }
    }

    pub(crate) fn from_pulldown_reference(
        kind: LinkSourceKind,
        dest: Cow<'a, str>,
        title: Cow<'a, str>,
        label: Cow<'a, str>,
    ) -> Self {
        let handle = ReferenceHandle::new(label);
        let source = match kind {
            LinkSourceKind::Inline => LinkSource::Inline,
            LinkSourceKind::ReferenceFull => LinkSource::ReferenceFull(handle),
            LinkSourceKind::ReferenceCollapsed => LinkSource::ReferenceCollapsed(handle),
            LinkSourceKind::ReferenceShortcut => LinkSource::ReferenceShortcut(handle),
        };
        Self {
            dest,
            title,
            source,
        }
    }

    pub(crate) fn dest(&self) -> &str {
        &self.dest
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    #[cfg(test)]
    pub(crate) fn source(&self) -> &LinkSource<'a> {
        &self.source
    }

    /// `Some(label)` for the three reference forms, `None` for inline.
    #[cfg(test)]
    pub(crate) fn label(&self) -> Option<&str> {
        self.source.handle().map(ReferenceHandle::label)
    }

    #[tracing::instrument(level = "trace", skip(self, ctx))]
    pub(crate) fn emit_style<'s>(&'s self, ctx: &LinkResolveCtx<'_>) -> EmitLinkStyle<'s> {
        decide_style(&self.source, ctx.body_text)
    }
}

impl<'a> ImageRun<'a> {
    pub(crate) fn from_pulldown_inline(dest: Cow<'a, str>, title: Cow<'a, str>) -> Self {
        Self {
            dest,
            title,
            source: LinkSource::Inline,
        }
    }

    pub(crate) fn from_pulldown_reference(
        kind: LinkSourceKind,
        dest: Cow<'a, str>,
        title: Cow<'a, str>,
        label: Cow<'a, str>,
    ) -> Self {
        let handle = ReferenceHandle::new(label);
        let source = match kind {
            LinkSourceKind::Inline => LinkSource::Inline,
            LinkSourceKind::ReferenceFull => LinkSource::ReferenceFull(handle),
            LinkSourceKind::ReferenceCollapsed => LinkSource::ReferenceCollapsed(handle),
            LinkSourceKind::ReferenceShortcut => LinkSource::ReferenceShortcut(handle),
        };
        Self {
            dest,
            title,
            source,
        }
    }

    pub(crate) fn dest(&self) -> &str {
        &self.dest
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    #[tracing::instrument(level = "trace", skip(self, ctx))]
    pub(crate) fn emit_style<'s>(&'s self, ctx: &LinkResolveCtx<'_>) -> EmitLinkStyle<'s> {
        decide_style(&self.source, ctx.body_text)
    }
}

/// Single decision site for the link-style choice. Inline stays inline;
/// `ReferenceFull` stays full; `ReferenceCollapsed` / `ReferenceShortcut`
/// keep their form when the body text CM-normalises to the same string
/// as the label, and demote to `ReferenceFull` otherwise. The check
/// runs on the actually-emitted body text supplied by the walker, so
/// any post-format drift introduced by emphasis rewriting is folded in.
fn decide_style<'s>(source: &'s LinkSource<'_>, body_text: &str) -> EmitLinkStyle<'s> {
    match source {
        LinkSource::Inline => EmitLinkStyle::Inline,
        LinkSource::ReferenceFull(h) => EmitLinkStyle::ReferenceFull { label: h.label() },
        LinkSource::ReferenceCollapsed(h) => {
            if labels_match(body_text, h.label()) {
                EmitLinkStyle::ReferenceCollapsed
            } else {
                EmitLinkStyle::ReferenceFull { label: h.label() }
            }
        }
        LinkSource::ReferenceShortcut(h) => {
            if labels_match(body_text, h.label()) {
                EmitLinkStyle::ReferenceShortcut
            } else {
                EmitLinkStyle::ReferenceFull { label: h.label() }
            }
        }
    }
}

fn labels_match(a: &str, b: &str) -> bool {
    cm_normalise_label(a) == cm_normalise_label(b)
}

/// Flatten a [`Doc`] to a string for the body-vs-label identity check.
/// Soft and hard breaks become a single space — CM label normalisation
/// collapses internal whitespace anyway, so the difference does not
/// survive the next stage.
pub(crate) fn flatten_body_doc(doc: &Doc<'_>) -> String {
    let mut out = String::new();
    walk(doc, &mut out);
    out
}

fn walk(doc: &Doc<'_>, out: &mut String) {
    match doc {
        Doc::Text(s) => out.push_str(s),
        Doc::Line | Doc::HardLine => out.push(' '),
        Doc::Atomic(inner) => walk(inner, out),
        Doc::Concat(items) => {
            for item in items {
                walk(item, out);
            }
        }
    }
}

/// `CommonMark` label normalisation: trim leading/trailing whitespace,
/// collapse internal whitespace runs to a single ASCII space, then
/// Unicode-lowercase. Matches `cmark::Util::normalize_label`.
fn cm_normalise_label(s: &str) -> String {
    let trimmed = s.trim();
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_was_ws = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !prev_was_ws {
                out.push(' ');
                prev_was_ws = true;
            }
        } else {
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_was_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::doc::{concat, hard_line, line, text, unbreakable};

    fn cow(s: &str) -> Cow<'_, str> {
        Cow::Borrowed(s)
    }

    fn ctx(body: &str) -> LinkResolveCtx<'_> {
        LinkResolveCtx { body_text: body }
    }

    #[test]
    fn inline_link_keeps_inline() {
        let run = LinkRun::from_pulldown_inline(cow("https://example.com"), cow(""));
        assert!(matches!(run.emit_style(&ctx("text")), EmitLinkStyle::Inline));
        assert_eq!(run.dest(), "https://example.com");
        assert!(run.title().is_empty());
        assert!(run.label().is_none());
    }

    #[test]
    fn reference_full_stays_full() {
        let run = LinkRun::from_pulldown_reference(
            LinkSourceKind::ReferenceFull,
            cow("https://example.com"),
            cow(""),
            cow("bar"),
        );
        let style = run.emit_style(&ctx("body"));
        assert!(
            matches!(style, EmitLinkStyle::ReferenceFull { label } if label == "bar"),
            "got {style:?}"
        );
        assert_eq!(run.label(), Some("bar"));
    }

    #[test]
    fn collapsed_matches_keeps_collapsed() {
        let run = LinkRun::from_pulldown_reference(
            LinkSourceKind::ReferenceCollapsed,
            cow(""),
            cow(""),
            cow("foo"),
        );
        let style = run.emit_style(&ctx("foo"));
        assert!(matches!(style, EmitLinkStyle::ReferenceCollapsed));
    }

    #[test]
    fn collapsed_mismatch_demotes_to_full() {
        // Emphasis rewriting changed body from `_foo_` to `*foo*`; the
        // label is still `_foo_`, so collapsed cannot be used.
        let run = LinkRun::from_pulldown_reference(
            LinkSourceKind::ReferenceCollapsed,
            cow(""),
            cow(""),
            cow("_foo_"),
        );
        let style = run.emit_style(&ctx("*foo*"));
        assert!(matches!(style, EmitLinkStyle::ReferenceFull { label } if label == "_foo_"));
    }

    #[test]
    fn shortcut_matches_keeps_shortcut() {
        let run = LinkRun::from_pulldown_reference(
            LinkSourceKind::ReferenceShortcut,
            cow(""),
            cow(""),
            cow("foo"),
        );
        let style = run.emit_style(&ctx("foo"));
        assert!(matches!(style, EmitLinkStyle::ReferenceShortcut));
    }

    #[test]
    fn shortcut_mismatch_demotes_to_full() {
        let run = LinkRun::from_pulldown_reference(
            LinkSourceKind::ReferenceShortcut,
            cow(""),
            cow(""),
            cow("a"),
        );
        let style = run.emit_style(&ctx("b"));
        assert!(matches!(style, EmitLinkStyle::ReferenceFull { .. }));
    }

    #[test]
    fn image_run_uses_same_decision() {
        let run = ImageRun::from_pulldown_reference(
            LinkSourceKind::ReferenceShortcut,
            cow(""),
            cow(""),
            cow("alt"),
        );
        assert!(matches!(
            run.emit_style(&ctx("alt")),
            EmitLinkStyle::ReferenceShortcut
        ));
        assert!(matches!(
            run.emit_style(&ctx("other")),
            EmitLinkStyle::ReferenceFull { .. }
        ));
    }

    #[test]
    fn cm_normalise_collapses_internal_ws() {
        assert_eq!(cm_normalise_label("Foo  Bar\tBaz"), "foo bar baz");
    }

    #[test]
    fn cm_normalise_trims_edges() {
        assert_eq!(cm_normalise_label("  hello  "), "hello");
    }

    #[test]
    fn cm_normalise_lowercases_ascii() {
        assert_eq!(cm_normalise_label("XYZ"), "xyz");
    }

    #[test]
    fn flatten_treats_breaks_as_space() {
        let doc = concat([text("foo"), line(), text("bar"), hard_line(), text("baz")]);
        // After flatten: "foo bar baz"; after CM-normalise: "foo bar baz".
        assert_eq!(flatten_body_doc(&doc), "foo bar baz");
    }

    #[test]
    fn flatten_descends_into_atomic() {
        let doc = concat([text("a"), unbreakable(text("b")), text("c")]);
        assert_eq!(flatten_body_doc(&doc), "abc");
    }

    #[test]
    fn hard_break_in_label_normalises_to_space() {
        // Body Doc carries a hard break inside the shortcut label; the
        // label string in the source is `"foo bar"`. After flatten +
        // normalise both sides become `"foo bar"`, so the shortcut
        // form survives.
        let body_doc = concat([text("foo"), hard_line(), text("bar")]);
        let body = flatten_body_doc(&body_doc);
        let run = LinkRun::from_pulldown_reference(
            LinkSourceKind::ReferenceShortcut,
            cow(""),
            cow(""),
            cow("foo bar"),
        );
        assert!(matches!(
            run.emit_style(&ctx(&body)),
            EmitLinkStyle::ReferenceShortcut
        ));
    }
}
