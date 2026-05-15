//! Inline-content walker: `NodeKind::*` → `Doc<'a>`.
//!
//! Text runs, code spans, and inline HTML are already in final-form
//! bytes by the time they reach this walker; the IR builder typed
//! them at construction. This module composes the typed leaves with
//! the structural inline kinds (`Emphasis`, `Strong`, `Strikethrough`,
//! `Link`, `Image`, `Autolink`, `FootnoteReference`).
//!
//! Emphasis delimiter normalisation comes from
//! [`crate::config::FmtOptions::resolve_italic`]. Link and image
//! destination/title escaping is destination-specific and lives in
//! this file because no other module needs it.

use std::borrow::Cow;

use crate::cm::inline::run::{InlineRun, RunPart};
use crate::format::ctx::Ctx;
use crate::format::doc::{Doc, concat, hard_line, line, text, unbreakable};
use crate::tree::{LinkKind, NodeId, NodeKind};

/// Concatenate the inline children of `parent` into one `Doc`.
pub(crate) fn render_inline<'a>(ctx: &Ctx<'a>, parent: NodeId) -> Doc<'a> {
    let ids: Vec<NodeId> = ctx.tree.children(parent).collect();
    render_inline_nodes(ctx, &ids)
}

/// Render an arbitrary slice of sibling inline nodes. Used for list
/// items whose children mix inline leaves with no enclosing Paragraph.
pub(crate) fn render_inline_nodes<'a>(ctx: &Ctx<'a>, ids: &[NodeId]) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::with_capacity(ids.len());
    // Resolved emphasis-delimiter of the most recent sibling Emphasis,
    // or `None` if the previous sibling is not Emphasis. Used by
    // `render_emphasis` to flip a `_↔*` collision against an abutting
    // emphasis sibling without an O(N²) parent walk.
    let mut prev_emphasis_delim: Option<u8> = None;
    for &cid in ids {
        let Some(node) = ctx.tree.node(cid) else {
            continue;
        };
        match &node.kind {
            NodeKind::Run(run) => parts.push(emit_run(run)),
            NodeKind::CodeRun(code) => parts.push(unbreakable(text(code.as_str().to_owned()))),
            NodeKind::HtmlSpan(span) => parts.push(unbreakable(text(span.as_str().to_owned()))),
            NodeKind::Emphasis => {
                let (doc, delim) = render_emphasis(ctx, cid, prev_emphasis_delim);
                parts.push(doc);
                prev_emphasis_delim = Some(delim);
                continue;
            }
            NodeKind::Strong => parts.push(render_strong(ctx, cid)),
            NodeKind::Strikethrough => parts.push(render_strikethrough(ctx, cid)),
            NodeKind::Link { .. } => parts.push(render_link(ctx, cid)),
            NodeKind::Image { .. } => parts.push(render_image(ctx, cid)),
            NodeKind::Autolink { url, kind } => {
                parts.push(render_autolink(url.as_ref(), *kind));
            }
            NodeKind::FootnoteReference(label) => {
                parts.push(text(format!("[^{label}]")));
            }
            NodeKind::TaskListMarker(_) => {
                // The list-item renderer prepends `[x] ` / `[ ] `; skip
                // the leaf so we don't emit it twice.
            }
            // Structural and block kinds should not appear here in a
            // well-formed tree. Falling back to verbatim source keeps
            // the formatter robust against future pulldown additions.
            NodeKind::Document
            | NodeKind::Paragraph
            | NodeKind::Heading { .. }
            | NodeKind::BlockQuote
            | NodeKind::List { .. }
            | NodeKind::Item { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::HtmlBlock { .. }
            | NodeKind::ThematicBreak
            | NodeKind::Table { .. }
            | NodeKind::TableHead
            | NodeKind::TableRow
            | NodeKind::TableCell
            | NodeKind::FootnoteDefinition { .. }
            | NodeKind::LinkReferenceDefinition { .. }
            | NodeKind::Unknown { .. } => {
                debug_assert!(
                    matches!(&node.kind, NodeKind::Unknown { .. }),
                    "non-inline NodeKind reached render_inline_nodes: {:?}",
                    &node.kind
                );
                parts.push(text(ctx.tree.raw_text(cid)));
            }
        }
        prev_emphasis_delim = None;
    }
    concat(parts)
}

/// Emit one [`InlineRun`] as a sequence of `Doc` primitives. Text
/// segments and break markers map 1:1 to the Doc combinators; the
/// run already chose `<br/>` vs `\` + newline at construction.
fn emit_run<'a>(run: &InlineRun<'a>) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::with_capacity(run.parts().len());
    for part in run.parts() {
        match part {
            RunPart::Text(s) => parts.push(text(s.clone())),
            RunPart::SoftBreak => parts.push(line()),
            RunPart::HardLineBreak => parts.push(concat([text("\\"), hard_line()])),
            RunPart::HardBreakTag => parts.push(text("<br/>")),
        }
    }
    concat(parts)
}

// ============================================================
// Emphasis / Strong / Strikethrough
// ============================================================

fn render_emphasis<'a>(
    ctx: &Ctx<'a>,
    id: NodeId,
    prev_sibling_emphasis_delim: Option<u8>,
) -> (Doc<'a>, u8) {
    let source_delim = source_emphasis_delim(ctx, id);
    let mut delim = ctx.opts.resolve_italic(source_delim);
    if first_child_is_strong(ctx, id) {
        delim = if delim == b'_' { b'*' } else { b'_' };
    }
    if prev_sibling_emphasis_delim == Some(delim) {
        delim = if delim == b'_' { b'*' } else { b'_' };
    }
    let d: &'static str = if delim == b'_' { "_" } else { "*" };
    let inner = render_inline(ctx, id);
    (concat([text(d), inner, text(d)]), delim)
}

fn first_child_is_strong(ctx: &Ctx<'_>, id: NodeId) -> bool {
    ctx.tree
        .children(id)
        .next()
        .and_then(|i| ctx.tree.node(i))
        .is_some_and(|n| matches!(n.kind, NodeKind::Strong))
}

fn render_strong<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let source_delim = source_emphasis_delim(ctx, id);
    let mut delim = ctx.opts.resolve_italic(source_delim);
    if first_child_is_emphasis(ctx, id) {
        delim = if delim == b'_' { b'*' } else { b'_' };
    }
    let d: &'static str = if delim == b'_' { "__" } else { "**" };
    let inner = render_inline(ctx, id);
    concat([text(d), inner, text(d)])
}

fn first_child_is_emphasis(ctx: &Ctx<'_>, id: NodeId) -> bool {
    ctx.tree
        .children(id)
        .next()
        .and_then(|i| ctx.tree.node(i))
        .is_some_and(|n| matches!(n.kind, NodeKind::Emphasis))
}

fn render_strikethrough<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let inner = render_inline(ctx, id);
    concat([text("~~"), inner, text("~~")])
}

/// First `*` or `_` byte inside the node's raw source range — the
/// opening delimiter for an Emphasis / Strong node. Falls back to
/// `b'*'` when the range is empty or starts with an unexpected byte.
fn source_emphasis_delim(ctx: &Ctx<'_>, id: NodeId) -> u8 {
    let raw = ctx.tree.raw_text(id);
    raw.bytes()
        .find(|b| matches!(b, b'*' | b'_'))
        .unwrap_or(b'*')
}

// ============================================================
// Links and images
// ============================================================

struct LinkTarget<'a> {
    dest: &'a str,
    title: Option<&'a str>,
    ref_label: Option<&'a str>,
    kind: LinkKind,
}

impl<'a> LinkTarget<'a> {
    fn from_node(kind: &'a NodeKind<'a>) -> Option<Self> {
        let (dest, title, ref_label, link_kind) = match kind {
            NodeKind::Link {
                dest,
                title,
                ref_label,
                kind,
            }
            | NodeKind::Image {
                dest,
                title,
                ref_label,
                kind,
            } => (dest, title, ref_label, *kind),
            NodeKind::Document
            | NodeKind::Paragraph
            | NodeKind::Heading { .. }
            | NodeKind::BlockQuote
            | NodeKind::List { .. }
            | NodeKind::Item { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::HtmlBlock { .. }
            | NodeKind::ThematicBreak
            | NodeKind::Table { .. }
            | NodeKind::TableHead
            | NodeKind::TableRow
            | NodeKind::TableCell
            | NodeKind::FootnoteDefinition { .. }
            | NodeKind::LinkReferenceDefinition { .. }
            | NodeKind::Run(_)
            | NodeKind::CodeRun(_)
            | NodeKind::Emphasis
            | NodeKind::Strong
            | NodeKind::Strikethrough
            | NodeKind::Autolink { .. }
            | NodeKind::HtmlSpan(_)
            | NodeKind::FootnoteReference(_)
            | NodeKind::TaskListMarker(_)
            | NodeKind::Unknown { .. } => return None,
        };
        let title = if title.is_empty() {
            None
        } else {
            Some(title.as_ref())
        };
        let ref_label = ref_label.as_deref();
        Some(Self {
            dest: dest.as_ref(),
            title,
            ref_label,
            kind: link_kind,
        })
    }
}

fn render_link<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    render_link_or_image(ctx, id, false)
}

fn render_image<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    render_link_or_image(ctx, id, true)
}

fn render_link_or_image<'a>(ctx: &Ctx<'a>, id: NodeId, is_image: bool) -> Doc<'a> {
    let Some(node) = ctx.tree.node(id) else {
        return concat([]);
    };
    let Some(target) = LinkTarget::from_node(&node.kind) else {
        return text(ctx.tree.raw_text(id));
    };

    let text_doc = render_inline(ctx, id);

    // For collapsed/shortcut forms, fall back to the explicit-label
    // form when the rendered text no longer matches the original
    // ref_label under CommonMark label normalisation.
    let effective_kind = match (target.kind, target.ref_label) {
        (LinkKind::ReferenceCollapsed | LinkKind::ReferenceShortcut, Some(label)) => {
            let rendered = render_doc_to_label_string(&text_doc);
            if cm_normalise_label(&rendered) == cm_normalise_label(label) {
                target.kind
            } else {
                LinkKind::ReferenceFull
            }
        }
        (k, _) => k,
    };

    let prefix = if is_image { "![" } else { "[" };

    match effective_kind {
        LinkKind::Inline => {
            let dest_doc = render_url_destination(target.dest, ctx.opts.link_def_style());
            let mut parts: Vec<Doc<'a>> = Vec::with_capacity(6);
            parts.push(text(prefix));
            parts.push(text_doc);
            parts.push(text("]("));
            parts.push(dest_doc);
            if let Some(t) = target.title {
                parts.push(text(format!(" \"{}\"", escape_title(t))));
            }
            parts.push(text(")"));
            unbreakable(concat(parts))
        }
        LinkKind::ReferenceFull => {
            let label = target.ref_label.unwrap_or("");
            unbreakable(concat([
                text(prefix),
                text_doc,
                text(format!("][{label}]")),
            ]))
        }
        LinkKind::ReferenceCollapsed => unbreakable(concat([text(prefix), text_doc, text("][]")])),
        LinkKind::ReferenceShortcut => unbreakable(concat([text(prefix), text_doc, text("]")])),
    }
}

/// Flatten a `Doc` to a string for the collapsed/shortcut identity
/// check. Soft breaks (`Doc::Line`) become a single space — that is
/// what CM normalisation does to internal whitespace anyway.
fn render_doc_to_label_string(doc: &Doc<'_>) -> String {
    let mut out = String::new();
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
    walk(doc, &mut out);
    out
}

/// `CommonMark` label normalisation (`cmark::Util::normalize_label`):
/// trim leading/trailing whitespace, collapse internal whitespace
/// runs to a single ASCII space, then ASCII-lowercase.
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

/// Render a URL destination, choosing between the bare and angle
/// forms. Returns a `Doc::Text` ready to splice into the link.
fn render_url_destination(url: &str, style: crate::config::LinkDefStyle) -> Doc<'_> {
    if matches!(style, crate::config::LinkDefStyle::Angle) {
        return text(format!("<{}>", escape_angle_url(url)));
    }
    match escape_url(url) {
        EscapedUrl::Bare(s) => text(s),
        EscapedUrl::Angle(s) => text(format!("<{s}>")),
    }
}

enum EscapedUrl<'a> {
    Bare(Cow<'a, str>),
    Angle(Cow<'a, str>),
}

/// CM §6.3 link destination escape. Prefer bare; switch to angle if
/// the URL contains whitespace. Inside the bare form, backslash-
/// escape parens that don't balance with a partner.
fn escape_url(url: &str) -> EscapedUrl<'_> {
    if url
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
    {
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

// ============================================================
// Autolinks
// ============================================================

fn render_autolink<'a>(url: &str, _kind: crate::tree::AutolinkKind) -> Doc<'a> {
    unbreakable(text(format!("<{url}>")))
}

#[cfg(test)]
mod tests {
    use super::{EscapedUrl, cm_normalise_label, escape_bare_url, escape_title, escape_url};

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
    fn escape_bare_url_balanced_parens_pass_through() {
        let out = escape_bare_url("https://en.wikipedia.org/wiki/Foo_(bar)");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn escape_bare_url_unbalanced_open_is_escaped() {
        let out = escape_bare_url("a(b");
        assert_eq!(out, "a\\(b");
    }

    #[test]
    fn escape_bare_url_unbalanced_close_is_escaped() {
        let out = escape_bare_url("a)b");
        assert_eq!(out, "a\\)b");
    }

    #[test]
    fn escape_url_with_space_picks_angle_form() {
        let out = escape_url("a b/c");
        assert!(matches!(&out, EscapedUrl::Angle(s) if s == "a b/c"));
    }

    #[test]
    fn escape_url_no_special_picks_bare_borrowed() {
        let out = escape_url("https://example.com/x");
        assert!(matches!(
            out,
            EscapedUrl::Bare(std::borrow::Cow::Borrowed(_))
        ));
    }

    #[test]
    fn escape_title_no_special_borrows() {
        let out = escape_title("plain title");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn escape_title_quote_and_backslash() {
        let out = escape_title(r#"a "quote" \ here"#);
        assert_eq!(out, r#"a \"quote\" \\ here"#);
    }
}
