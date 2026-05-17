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
//! the infallible `from_pulldown_inline` constructor for inline links,
//! and the fallible [`LinkRun::try_new_reference`] /
//! [`ImageRun::try_new_reference`] for reference-style links. The
//! reference constructor resolves the label against a
//! [`ReferenceTable`](crate::cm::refs::ReferenceTable) at IR-build
//! time; unresolvable labels return [`LinkError::UnresolvedReference`]
//! and the builder downgrades them to raw text per CM §4.7's
//! "leave as text" rule. The format walker
//! ([`crate::format::inline`]) renders the body, flattens it via
//! [`flatten_body_doc`], and calls `emit_style` once — the sole site
//! that decides the final style.

use std::borrow::Cow;

#[cfg(test)]
use crate::cm::refs::ReferenceTable;
use crate::format::doc::Doc;

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

    #[tracing::instrument(level = "trace", skip(self, ctx))]
    pub(crate) fn emit_style<'s>(&'s self, ctx: &LinkResolveCtx<'_>) -> EmitLinkStyle<'s> {
        decide_style(&self.source, ctx.body_text)
    }

    /// Emit this link with the resolved style.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self, body: Doc<'b>, ctx: &crate::format::pretty::PrettyCtx<'b>) -> Doc<'b> {
        let flat = flatten_body_doc(&body);
        let style = self.emit_style(&LinkResolveCtx { body_text: &flat });
        assemble_link(ctx, body, self.dest(), self.title(), &style, false)
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

    #[tracing::instrument(level = "trace", skip(self, ctx))]
    pub(crate) fn emit_style<'s>(&'s self, ctx: &LinkResolveCtx<'_>) -> EmitLinkStyle<'s> {
        decide_style(&self.source, ctx.body_text)
    }

    /// Emit this image with the resolved style.
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'b>(&self, body: Doc<'b>, ctx: &crate::format::pretty::PrettyCtx<'b>) -> Doc<'b> {
        let flat = flatten_body_doc(&body);
        let style = self.emit_style(&LinkResolveCtx { body_text: &flat });
        assemble_link(ctx, body, self.dest(), self.title(), &style, true)
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

/// Single decision site for the link-style choice. Inline stays inline;
/// `ReferenceFull` stays full; `ReferenceCollapsed` / `ReferenceShortcut`
/// keep their form when the body text CM-normalises to the same string
/// as the label, and demote to `ReferenceFull` otherwise. The check
/// runs on the actually-emitted body text supplied by the walker, so
/// any post-format drift introduced by emphasis rewriting is folded in.
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

/// Flatten a [`Doc`] to a string for the body-vs-label identity check.
/// Soft and hard breaks become a single space — CM label normalisation
/// collapses internal whitespace anyway, so the difference does not
/// survive the next stage.
pub(crate) fn flatten_body_doc(doc: &Doc<'_>) -> String {
    let mut out = String::new();
    walk(doc, &mut out);
    out
}

// Iterative to bound stack usage on adversarial inputs with deeply
// nested `Doc::Concat` / `Doc::Atomic` chains.
fn walk(doc: &Doc<'_>, out: &mut String) {
    let mut stack: Vec<&Doc<'_>> = vec![doc];
    while let Some(node) = stack.pop() {
        match node {
            Doc::Text(s) => out.push_str(s),
            Doc::Line | Doc::SoftSpace | Doc::HardLine => out.push(' '),
            Doc::Atomic(inner) | Doc::Prefix(_, inner) => stack.push(inner),
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    stack.push(item);
                }
            }
        }
    }
}

// ============================================================
// Link / image assembly
// ============================================================

/// Shared between [`LinkRun::pretty`] and [`ImageRun::pretty`]:
/// emits the body wrapped in `[…](…)` (inline), `[…][label]` (full),
/// `[…][]` (collapsed), or `[…]` (shortcut) per `style`.
fn assemble_link<'a>(
    ctx: &crate::format::pretty::PrettyCtx<'a>,
    body_doc: Doc<'a>,
    dest: &str,
    title: &str,
    style: &EmitLinkStyle<'_>,
    is_image: bool,
) -> Doc<'a> {
    use crate::format::doc::{concat, text, unbreakable};
    let prefix = if is_image { "![" } else { "[" };
    match style {
        EmitLinkStyle::Inline => {
            let dest_str = render_url_destination_owned(dest, ctx.opts.link_def_style());
            let mut parts: Vec<Doc<'a>> = Vec::with_capacity(6);
            parts.push(text(prefix));
            parts.push(body_doc);
            parts.push(text("]("));
            parts.push(text(dest_str));
            if !title.is_empty() {
                parts.push(text(format!(" \"{}\"", escape_title(title))));
            }
            parts.push(text(")"));
            unbreakable(concat(parts))
        }
        EmitLinkStyle::ReferenceFull { label } => {
            unbreakable(concat([text(prefix), body_doc, text(format!("][{label}]"))]))
        }
        EmitLinkStyle::ReferenceCollapsed => unbreakable(concat([text(prefix), body_doc, text("][]")])),
        EmitLinkStyle::ReferenceShortcut => unbreakable(concat([text(prefix), body_doc, text("]")])),
    }
}

/// Render a URL destination, choosing between the bare and angle
/// forms. Public so the link-reference-definition emitter in
/// `format/document.rs` can share the same escape policy.
pub(crate) fn render_url_destination_owned(url: &str, style: crate::config::LinkDefStyle) -> String {
    if matches!(style, crate::config::LinkDefStyle::Angle) {
        return format!("<{}>", escape_angle_url(url));
    }
    match escape_url(url) {
        EscapedUrl::Bare(s) => s.into_owned(),
        EscapedUrl::Angle(s) => format!("<{s}>"),
    }
}

enum EscapedUrl<'a> {
    Bare(Cow<'a, str>),
    Angle(Cow<'a, str>),
}

fn escape_url(url: &str) -> EscapedUrl<'_> {
    if url.bytes().any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')) {
        return EscapedUrl::Angle(escape_angle_url(url));
    }
    EscapedUrl::Bare(escape_bare_url(url))
}

fn escape_bare_url(url: &str) -> Cow<'_, str> {
    let bytes = url.as_bytes();
    let mut needs_escape: Vec<bool> = vec![false; bytes.len()];
    let mut open_stack: Vec<usize> = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => open_stack.push(i),
            b')' => {
                if open_stack.pop().is_none()
                    && let Some(slot) = needs_escape.get_mut(i)
                {
                    *slot = true;
                }
            }
            _ => {}
        }
    }
    for i in &open_stack {
        if let Some(slot) = needs_escape.get_mut(*i) {
            *slot = true;
        }
    }
    let any = needs_escape.iter().any(|&b| b);
    if !any {
        return Cow::Borrowed(url);
    }
    let mut out = String::with_capacity(url.len().saturating_add(open_stack.len()));
    for (i, &b) in bytes.iter().enumerate() {
        if needs_escape.get(i).copied().unwrap_or(false) {
            out.push('\\');
        }
        out.push(char::from(b));
    }
    Cow::Owned(out)
}

fn escape_angle_url(url: &str) -> Cow<'_, str> {
    if url.bytes().all(|b| !matches!(b, b'<' | b'>' | b'\\')) {
        return Cow::Borrowed(url);
    }
    let mut out = String::with_capacity(url.len().saturating_add(4));
    for ch in url.chars() {
        match ch {
            '<' | '>' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    Cow::Owned(out)
}

fn escape_title(title: &str) -> Cow<'_, str> {
    if title.bytes().all(|b| !matches!(b, b'\\' | b'"')) {
        return Cow::Borrowed(title);
    }
    let mut out = String::with_capacity(title.len().saturating_add(4));
    for ch in title.chars() {
        match ch {
            '\\' | '"' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    Cow::Owned(out)
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
    use crate::format::doc::{concat, hard_line, line, text, unbreakable};

    fn cow(s: &str) -> String {
        s.to_owned()
    }

    fn ctx(body: &str) -> LinkResolveCtx<'_> {
        LinkResolveCtx { body_text: body }
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
        assert!(matches!(run.emit_style(&ctx("text")), EmitLinkStyle::Inline));
        assert_eq!(run.dest(), "https://example.com");
        assert!(run.title().is_empty());
        assert!(run.label().is_none());
    }

    #[test]
    fn reference_full_stays_full() {
        let table = table_with("bar");
        let run = LinkRun::try_new_reference(
            LinkSourceKind::ReferenceFull,
            cow("https://example.com"),
            cow(""),
            cow("bar"),
            &table,
        )
        .expect("resolves");
        let style = run.emit_style(&ctx("body"));
        assert!(
            matches!(style, EmitLinkStyle::ReferenceFull { label } if label == "bar"),
            "got {style:?}"
        );
        assert_eq!(run.label(), Some("bar"));
    }

    #[test]
    fn collapsed_matches_keeps_collapsed() {
        let table = table_with("foo");
        let run = LinkRun::try_new_reference(LinkSourceKind::ReferenceCollapsed, cow(""), cow(""), cow("foo"), &table)
            .expect("resolves");
        let style = run.emit_style(&ctx("foo"));
        assert!(matches!(style, EmitLinkStyle::ReferenceCollapsed));
    }

    #[test]
    fn collapsed_mismatch_demotes_to_full() {
        // Emphasis rewriting changed body from `_foo_` to `*foo*`; the
        // label is still `_foo_`, so collapsed cannot be used.
        let table = table_with("_foo_");
        let run = LinkRun::try_new_reference(
            LinkSourceKind::ReferenceCollapsed,
            cow(""),
            cow(""),
            cow("_foo_"),
            &table,
        )
        .expect("resolves");
        let style = run.emit_style(&ctx("*foo*"));
        assert!(matches!(style, EmitLinkStyle::ReferenceFull { label } if label == "_foo_"));
    }

    #[test]
    fn shortcut_matches_keeps_shortcut() {
        let table = table_with("foo");
        let run = LinkRun::try_new_reference(LinkSourceKind::ReferenceShortcut, cow(""), cow(""), cow("foo"), &table)
            .expect("resolves");
        let style = run.emit_style(&ctx("foo"));
        assert!(matches!(style, EmitLinkStyle::ReferenceShortcut));
    }

    #[test]
    fn shortcut_mismatch_demotes_to_full() {
        let table = table_with("a");
        let run = LinkRun::try_new_reference(LinkSourceKind::ReferenceShortcut, cow(""), cow(""), cow("a"), &table)
            .expect("resolves");
        let style = run.emit_style(&ctx("b"));
        assert!(matches!(style, EmitLinkStyle::ReferenceFull { .. }));
    }

    #[test]
    fn image_run_uses_same_decision() {
        let table = table_with("alt");
        let run = ImageRun::try_new_reference(LinkSourceKind::ReferenceShortcut, cow(""), cow(""), cow("alt"), &table)
            .expect("resolves");
        assert!(matches!(run.emit_style(&ctx("alt")), EmitLinkStyle::ReferenceShortcut));
        assert!(matches!(
            run.emit_style(&ctx("other")),
            EmitLinkStyle::ReferenceFull { .. }
        ));
    }

    #[test]
    fn unresolved_reference_errors() {
        let table = crate::cm::refs::ReferenceTable::empty();
        let err = LinkRun::try_new_reference(LinkSourceKind::ReferenceFull, cow(""), cow(""), cow("missing"), &table)
            .unwrap_err();
        assert_eq!(err, LinkError::UnresolvedReference);
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
        let body_doc = concat([text("foo"), hard_line(), text("bar")]);
        let body = flatten_body_doc(&body_doc);
        let table = table_with("foo bar");
        let run = LinkRun::try_new_reference(
            LinkSourceKind::ReferenceShortcut,
            cow(""),
            cow(""),
            cow("foo bar"),
            &table,
        )
        .expect("resolves");
        assert!(matches!(run.emit_style(&ctx(&body)), EmitLinkStyle::ReferenceShortcut));
    }
}
