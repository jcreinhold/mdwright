//! Typed link / image values.
//!
//! [`LinkRun`] and [`ImageRun`] capture the parse-time data pulldown saw
//! and emit one source-form per CM grammar variant: inline stays inline,
//! `ReferenceFull` stays full, `ReferenceCollapsed` stays collapsed,
//! `ReferenceShortcut` stays shortcut. No `FmtOptions` style knob is
//! consulted; under preserve defaults the body bytes also round-trip,
//! so the collapsed / shortcut identity is preserved by construction
//! (the body bytes that came from source still equal the source
//! label).
//!
//! For inline links, the URL destination's source form (bare `url` vs
//! angle-bracketed `<url>`) is recovered at emit time from the link's
//! source range. Reference-definition emit (see
//! [`crate::format::block`]) lacks a tracked source range for the
//! destination and falls back to the existing `escape_url` inference.
//!
//! The IR builder ([`crate::tree::TreeBuilder`]) constructs values via
//! the infallible `from_pulldown_inline` constructor for inline links,
//! and the fallible [`LinkRun::try_new_reference`] /
//! [`ImageRun::try_new_reference`] for reference-style links. The
//! reference constructor resolves the label against a
//! [`ReferenceTable`](crate::cm::refs::ReferenceTable) at IR-build
//! time; unresolvable labels return [`LinkError::UnresolvedReference`]
//! and the builder downgrades them to raw text per CM §4.7's
//! "leave as text" rule.

#![allow(dead_code)]
#[cfg(test)]
use crate::cm::refs::ReferenceTable;

/// Source CM grammar variant for a reference link. Inline links never
/// reach this enum — production constructs them via
/// `from_pulldown_inline` directly. The `Reference` prefix is
/// intentional and parallels [`LinkSource`]'s variant names: clippy's
/// "all variants share a prefix" suggestion would erase the structural
/// correspondence.
#[allow(clippy::enum_variant_names)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkSourceKind {
    ReferenceFull,
    ReferenceCollapsed,
    ReferenceShortcut,
}

/// Source-side data for the link or image, mirroring the four CM
/// grammar variants. Inline carries no handle; the three reference
/// forms each carry a [`ResolvedRef`] — a [`ReferenceHandle`] paired
/// with the raw label as the source wrote it.
#[derive(Clone, Debug)]
pub(crate) enum LinkSource {
    Inline,
    ReferenceFull(ResolvedRef),
    ReferenceCollapsed(ResolvedRef),
    ReferenceShortcut(ResolvedRef),
}

impl LinkSource {
    /// `None` for [`LinkSource::Inline`] (which has no reference
    /// kind); `Some(kind)` for the three reference variants. Test-only:
    /// production code dispatches on the variants directly.
    #[cfg(test)]
    pub(crate) fn kind(&self) -> Option<LinkSourceKind> {
        match self {
            Self::Inline => None,
            Self::ReferenceFull(_) => Some(LinkSourceKind::ReferenceFull),
            Self::ReferenceCollapsed(_) => Some(LinkSourceKind::ReferenceCollapsed),
            Self::ReferenceShortcut(_) => Some(LinkSourceKind::ReferenceShortcut),
        }
    }

    fn resolved(&self) -> Option<&ResolvedRef> {
        match self {
            Self::Inline => None,
            Self::ReferenceFull(r) | Self::ReferenceCollapsed(r) | Self::ReferenceShortcut(r) => Some(r),
        }
    }
}

/// A reference-style link that has been resolved against the
/// document's [`ReferenceTable`]. The `label` field is what the source
/// wrote between the brackets, used verbatim for `[label]` emission.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedRef {
    label: String,
}

impl ResolvedRef {
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

/// A reference-style link that could not be resolved against the
/// document's [`ReferenceTable`]. Used only by the test-only
/// [`LinkRun::try_new_reference`] /
/// [`ImageRun::try_new_reference`] helpers; the production path
/// uses [`LinkRun::from_pulldown_reference`] + post-pass validation
/// and downgrades unresolved labels to raw source emission instead
/// of producing an error.
#[cfg(test)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkError {
    UnresolvedReference,
}

/// Typed inline link.
#[derive(Clone, Debug)]
pub struct LinkRun {
    dest: String,
    title: String,
    source: LinkSource,
}

/// Typed inline image. `dest` is the image URL pulldown extracted at
/// parse time; the format walker re-emits it via the same URL-escape
/// path as [`LinkRun`].
#[derive(Clone, Debug)]
pub struct ImageRun {
    dest: String,
    title: String,
    source: LinkSource,
}

/// Final emission style — one variant per `LinkSource` variant.
/// Carries the label borrow when the chosen variant needs one, so the
/// walker does not reach back into the source to retrieve it.
#[derive(Debug)]
pub(crate) enum EmitLinkStyle<'a> {
    Inline,
    ReferenceFull { label: &'a str },
    ReferenceCollapsed,
    ReferenceShortcut,
}

impl LinkRun {
    pub(crate) fn from_pulldown_inline(dest: String, title: String) -> Self {
        Self {
            dest,
            title,
            source: LinkSource::Inline,
        }
    }

    /// Construct a reference-style link without validating the label
    /// against the document's [`ReferenceTable`]. The tree builder
    /// calls this during the pulldown event walk — the table is not
    /// yet built at that point because it depends on the code-block
    /// ranges the walk discovers. [`TreeBuilder::finalize`] does the
    /// validation post-pass: nodes whose label fails to resolve are
    /// downgraded to raw-source emission.
    pub(crate) fn from_pulldown_reference(kind: LinkSourceKind, dest: String, title: String, label: String) -> Self {
        let resolved = ResolvedRef { label };
        let source = match kind {
            LinkSourceKind::ReferenceFull => LinkSource::ReferenceFull(resolved),
            LinkSourceKind::ReferenceCollapsed => LinkSource::ReferenceCollapsed(resolved),
            LinkSourceKind::ReferenceShortcut => LinkSource::ReferenceShortcut(resolved),
        };
        Self { dest, title, source }
    }

    /// Resolve a reference-style link against `table`. Returns
    /// [`LinkError::UnresolvedReference`] when the label is missing.
    /// Test-only: production goes through
    /// [`Self::from_pulldown_reference`] + post-pass validation, which
    /// downgrades unresolved labels to raw-source emission rather than
    /// erroring.
    #[cfg(test)]
    #[tracing::instrument(level = "trace", skip(table))]
    pub(crate) fn try_new_reference(
        kind: LinkSourceKind,
        dest: String,
        title: String,
        label: String,
        table: &ReferenceTable,
    ) -> Result<Self, LinkError> {
        let source = resolve_kind(kind, label, table)?;
        Ok(Self { dest, title, source })
    }

    /// Inspect the reference label, if any. Returns `None` for inline
    /// links. Used by the finalize validation pass to look the label
    /// up in the [`ReferenceTable`].
    pub(crate) fn reference_label(&self) -> Option<&str> {
        self.source.resolved().map(ResolvedRef::label)
    }

    pub(crate) fn dest(&self) -> &str {
        &self.dest
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    #[cfg(test)]
    pub(crate) fn source(&self) -> &LinkSource {
        &self.source
    }

    /// `Some(label)` for the three reference forms, `None` for inline.
    #[cfg(test)]
    pub(crate) fn label(&self) -> Option<&str> {
        self.source.resolved().map(ResolvedRef::label)
    }

    /// Choose the emit style from the source variant. `body_text` is
    /// the rendered body's flattened bytes; for Collapsed/Shortcut
    /// these must CM-normalise to the source label, otherwise the
    /// link is demoted to Full so the re-parse still resolves. Never
    /// consults `FmtOptions`.
    #[tracing::instrument(level = "trace", skip(self, body_text))]
    pub(crate) fn emit_style<'s>(&'s self, body_text: &str) -> EmitLinkStyle<'s> {
        decide_style(&self.source, body_text)
    }
}

impl ImageRun {
    pub(crate) fn from_pulldown_inline(dest: String, title: String) -> Self {
        Self {
            dest,
            title,
            source: LinkSource::Inline,
        }
    }

    /// Image counterpart to [`LinkRun::from_pulldown_reference`].
    pub(crate) fn from_pulldown_reference(kind: LinkSourceKind, dest: String, title: String, label: String) -> Self {
        let resolved = ResolvedRef { label };
        let source = match kind {
            LinkSourceKind::ReferenceFull => LinkSource::ReferenceFull(resolved),
            LinkSourceKind::ReferenceCollapsed => LinkSource::ReferenceCollapsed(resolved),
            LinkSourceKind::ReferenceShortcut => LinkSource::ReferenceShortcut(resolved),
        };
        Self { dest, title, source }
    }

    /// Image counterpart to [`LinkRun::try_new_reference`]. Test-only.
    #[cfg(test)]
    #[tracing::instrument(level = "trace", skip(table))]
    pub(crate) fn try_new_reference(
        kind: LinkSourceKind,
        dest: String,
        title: String,
        label: String,
        table: &ReferenceTable,
    ) -> Result<Self, LinkError> {
        let source = resolve_kind(kind, label, table)?;
        Ok(Self { dest, title, source })
    }

    pub(crate) fn dest(&self) -> &str {
        &self.dest
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    /// Inspect the reference label, if any.
    pub(crate) fn reference_label(&self) -> Option<&str> {
        self.source.resolved().map(ResolvedRef::label)
    }

    /// See [`LinkRun::emit_style`].
    #[tracing::instrument(level = "trace", skip(self, body_text))]
    pub(crate) fn emit_style<'s>(&'s self, body_text: &str) -> EmitLinkStyle<'s> {
        decide_style(&self.source, body_text)
    }
}

#[cfg(test)]
fn resolve_kind(kind: LinkSourceKind, label: String, table: &ReferenceTable) -> Result<LinkSource, LinkError> {
    table.resolve(&label).ok_or(LinkError::UnresolvedReference)?;
    let resolved = ResolvedRef { label };
    Ok(match kind {
        LinkSourceKind::ReferenceFull => LinkSource::ReferenceFull(resolved),
        LinkSourceKind::ReferenceCollapsed => LinkSource::ReferenceCollapsed(resolved),
        LinkSourceKind::ReferenceShortcut => LinkSource::ReferenceShortcut(resolved),
    })
}

/// Single decision site for the link-style choice. Inline → Inline,
/// `ReferenceFull` → Full. `Collapsed` / `Shortcut` *normally* echo
/// the source variant, but if the rendered body bytes' CM-normalised
/// form no longer equals the source label's CM-normalised form, the
/// link must be demoted to `ReferenceFull` — otherwise the
/// collapsed/shortcut emit won't re-resolve. The mismatch is
/// structural rather than stylistic: the inline escape policy (e.g.
/// emphasis-safety `\_`) can perturb body bytes inside link text in
/// ways that survive HTML-equivalence but change CM label resolution.
fn decide_style<'s>(source: &'s LinkSource, body_text: &str) -> EmitLinkStyle<'s> {
    match source {
        LinkSource::Inline => EmitLinkStyle::Inline,
        LinkSource::ReferenceFull(r) => EmitLinkStyle::ReferenceFull { label: r.label() },
        LinkSource::ReferenceCollapsed(r) => {
            if labels_match(body_text, r.label()) {
                EmitLinkStyle::ReferenceCollapsed
            } else {
                EmitLinkStyle::ReferenceFull { label: r.label() }
            }
        }
        LinkSource::ReferenceShortcut(r) => {
            if labels_match(body_text, r.label()) {
                EmitLinkStyle::ReferenceShortcut
            } else {
                EmitLinkStyle::ReferenceFull { label: r.label() }
            }
        }
    }
}

fn labels_match(a: &str, b: &str) -> bool {
    cm_normalise_label(a) == cm_normalise_label(b)
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
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::cm::refs::build_reference_table;

    fn cow(s: &str) -> String {
        s.to_owned()
    }

    /// Build a single-label table by parsing a one-def, one-link
    /// document so pulldown emits the resolved Reference event the
    /// table consumes.
    fn table_with(label: &str) -> crate::cm::refs::ReferenceTable {
        use crate::parse;
        use crate::source::{CanonicalSource, Source};
        let src = format!("[{label}]: https://example.com\n\n[{label}][{label}]\n");
        let source = Source::new(&src);
        let events: Vec<_> = parse::events(CanonicalSource::from_source(&source), parse::FORMATTER_OPTIONS).collect();
        build_reference_table(&events, &src)
    }

    #[test]
    fn inline_link_keeps_inline() {
        let run = LinkRun::from_pulldown_inline(cow("https://example.com"), cow(""));
        assert!(matches!(run.emit_style("text"), EmitLinkStyle::Inline));
        assert_eq!(run.dest(), "https://example.com");
        assert!(run.title().is_empty());
        assert!(run.label().is_none());
    }

    #[test]
    fn reference_full_emits_full() {
        let table = table_with("bar");
        let run = LinkRun::try_new_reference(
            LinkSourceKind::ReferenceFull,
            cow("https://example.com"),
            cow(""),
            cow("bar"),
            &table,
        )
        .expect("resolves");
        let style = run.emit_style("body");
        assert!(
            matches!(style, EmitLinkStyle::ReferenceFull { label } if label == "bar"),
            "got {style:?}"
        );
        assert_eq!(run.label(), Some("bar"));
    }

    #[test]
    fn reference_collapsed_emits_collapsed_when_body_matches_label() {
        let table = table_with("foo");
        let run = LinkRun::try_new_reference(LinkSourceKind::ReferenceCollapsed, cow(""), cow(""), cow("foo"), &table)
            .expect("resolves");
        assert!(matches!(run.emit_style("foo"), EmitLinkStyle::ReferenceCollapsed));
    }

    #[test]
    fn reference_collapsed_demotes_to_full_on_body_drift() {
        // Inline escape policy may emit `\_foo\_` from a body whose
        // source label was `_foo_`. Without demotion the emitted
        // `[\_foo\_][]` would not re-resolve.
        let table = table_with("_foo_");
        let run = LinkRun::try_new_reference(
            LinkSourceKind::ReferenceCollapsed,
            cow(""),
            cow(""),
            cow("_foo_"),
            &table,
        )
        .expect("resolves");
        let style = run.emit_style("*foo*");
        assert!(matches!(style, EmitLinkStyle::ReferenceFull { label } if label == "_foo_"));
    }

    #[test]
    fn reference_shortcut_emits_shortcut_when_body_matches_label() {
        let table = table_with("foo");
        let run = LinkRun::try_new_reference(LinkSourceKind::ReferenceShortcut, cow(""), cow(""), cow("foo"), &table)
            .expect("resolves");
        assert!(matches!(run.emit_style("foo"), EmitLinkStyle::ReferenceShortcut));
    }

    #[test]
    fn image_run_uses_same_decision() {
        let table = table_with("alt");
        let run = ImageRun::try_new_reference(LinkSourceKind::ReferenceShortcut, cow(""), cow(""), cow("alt"), &table)
            .expect("resolves");
        assert!(matches!(run.emit_style("alt"), EmitLinkStyle::ReferenceShortcut));
    }

    #[test]
    fn unresolved_reference_errors() {
        let table = crate::cm::refs::ReferenceTable::empty();
        let err = LinkRun::try_new_reference(LinkSourceKind::ReferenceFull, cow(""), cow(""), cow("missing"), &table)
            .unwrap_err();
        assert_eq!(err, LinkError::UnresolvedReference);
    }
}
