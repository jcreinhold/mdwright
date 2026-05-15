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

use crate::cm::inline::autolink::AutolinkRun;
use crate::cm::inline::emphasis::{EmphasisDelim, EmphasisRun, ResolveCtx, StrongRun};
use crate::cm::inline::link::{
    EmitLinkStyle, ImageRun, LinkResolveCtx, LinkRun, flatten_body_doc,
};
use crate::cm::inline::run::{InlineRun, RunPart};
use crate::format::ctx::Ctx;
use crate::format::doc::{Doc, concat, hard_line, line, text, unbreakable};
use crate::tree::{NodeId, NodeKind};

/// Concatenate the inline children of `parent` into one `Doc`.
pub(crate) fn render_inline<'a>(ctx: &Ctx<'a>, parent: NodeId) -> Doc<'a> {
    let ids: Vec<NodeId> = ctx.tree.children(parent).collect();
    render_inline_nodes(ctx, &ids)
}

/// Render an arbitrary slice of sibling inline nodes. Used for list
/// items whose children mix inline leaves with no enclosing Paragraph.
pub(crate) fn render_inline_nodes<'a>(ctx: &Ctx<'a>, ids: &[NodeId]) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::with_capacity(ids.len());
    // Resolved delimiter of the most recent sibling Emphasis, or
    // `None` if the previous sibling is not Emphasis. Threaded into
    // `EmphasisRun::resolve` as `left_sibling_delim` so the next
    // emphasis flips its delimiter when two abut.
    let mut left_emphasis_delim: Option<EmphasisDelim> = None;
    for &cid in ids {
        let Some(node) = ctx.tree.node(cid) else {
            continue;
        };
        match &node.kind {
            NodeKind::Run(run) => parts.push(emit_run(run)),
            NodeKind::CodeRun(code) => parts.push(unbreakable(text(code.as_str().to_owned()))),
            NodeKind::HtmlSpan(span) => parts.push(unbreakable(text(span.as_str().to_owned()))),
            NodeKind::Emphasis(run) => {
                let (doc, delim) = render_emphasis(ctx, cid, *run, left_emphasis_delim);
                parts.push(doc);
                left_emphasis_delim = Some(delim);
                continue;
            }
            NodeKind::Strong(run) => parts.push(render_strong(ctx, cid, *run)),
            NodeKind::Strikethrough => parts.push(render_strikethrough(ctx, cid)),
            NodeKind::Link(run) => parts.push(render_link(ctx, cid, run)),
            NodeKind::Image(run) => parts.push(render_image(ctx, cid, run)),
            NodeKind::Autolink(run) => parts.push(render_autolink(run)),
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
        left_emphasis_delim = None;
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
    run: EmphasisRun,
    left_sibling_delim: Option<EmphasisDelim>,
) -> (Doc<'a>, EmphasisDelim) {
    let delim = run.resolve(ResolveCtx {
        style: ctx.opts.italic(),
        left_sibling_delim,
        first_child_delim: first_child_strong_delim(ctx, id),
    });
    let d = delim.as_str();
    let inner = render_inline(ctx, id);
    (concat([text(d), inner, text(d)]), delim)
}

fn render_strong<'a>(ctx: &Ctx<'a>, id: NodeId, run: StrongRun) -> Doc<'a> {
    let delim = run.resolve(ResolveCtx {
        style: ctx.opts.italic(),
        left_sibling_delim: None,
        first_child_delim: first_child_emphasis_delim(ctx, id),
    });
    let d: &'static str = match delim {
        EmphasisDelim::Asterisk => "**",
        EmphasisDelim::Underscore => "__",
    };
    let inner = render_inline(ctx, id);
    concat([text(d), inner, text(d)])
}

/// `Some(d)` if the first child of `id` is a Strong run that will
/// resolve to delimiter `d`. Used by `render_emphasis` to flip the
/// outer delimiter so nested `*` / `**` do not fuse into `***`.
fn first_child_strong_delim(ctx: &Ctx<'_>, id: NodeId) -> Option<EmphasisDelim> {
    let first = ctx.tree.children(id).next()?;
    let node = ctx.tree.node(first)?;
    let NodeKind::Strong(run) = &node.kind else { return None };
    Some(run.resolve(ResolveCtx {
        style: ctx.opts.italic(),
        left_sibling_delim: None,
        first_child_delim: None,
    }))
}

/// Symmetric peer of [`first_child_strong_delim`] for the Strong
/// renderer: flips `**` to `__` when the first child is an Emphasis
/// that resolves to the same byte family.
fn first_child_emphasis_delim(ctx: &Ctx<'_>, id: NodeId) -> Option<EmphasisDelim> {
    let first = ctx.tree.children(id).next()?;
    let node = ctx.tree.node(first)?;
    let NodeKind::Emphasis(run) = &node.kind else { return None };
    Some(run.resolve(ResolveCtx {
        style: ctx.opts.italic(),
        left_sibling_delim: None,
        first_child_delim: None,
    }))
}

fn render_strikethrough<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let inner = render_inline(ctx, id);
    concat([text("~~"), inner, text("~~")])
}

// ============================================================
// Links and images
// ============================================================

fn render_link<'a>(ctx: &Ctx<'a>, id: NodeId, run: &LinkRun<'a>) -> Doc<'a> {
    let text_doc = render_inline(ctx, id);
    let flat = flatten_body_doc(&text_doc);
    let style = run.emit_style(&LinkResolveCtx { body_text: &flat });
    assemble_link(ctx, text_doc, run.dest(), run.title(), &style, /*is_image=*/ false)
}

fn render_image<'a>(ctx: &Ctx<'a>, id: NodeId, run: &ImageRun<'a>) -> Doc<'a> {
    let text_doc = render_inline(ctx, id);
    let flat = flatten_body_doc(&text_doc);
    let style = run.emit_style(&LinkResolveCtx { body_text: &flat });
    assemble_link(ctx, text_doc, run.dest(), run.title(), &style, /*is_image=*/ true)
}

fn assemble_link<'a>(
    ctx: &Ctx<'a>,
    body_doc: Doc<'a>,
    dest: &str,
    title: &str,
    style: &EmitLinkStyle<'_>,
    is_image: bool,
) -> Doc<'a> {
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
        EmitLinkStyle::ReferenceFull { label } => unbreakable(concat([
            text(prefix),
            body_doc,
            text(format!("][{label}]")),
        ])),
        EmitLinkStyle::ReferenceCollapsed => {
            unbreakable(concat([text(prefix), body_doc, text("][]")]))
        }
        EmitLinkStyle::ReferenceShortcut => {
            unbreakable(concat([text(prefix), body_doc, text("]")]))
        }
    }
}

/// Render a URL destination, choosing between the bare and angle
/// forms. Returns an owned string so the caller can splice it into a
/// `Doc<'a>` without lifetime gymnastics.
fn render_url_destination_owned(url: &str, style: crate::config::LinkDefStyle) -> String {
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

fn render_autolink<'a>(run: &AutolinkRun<'_>) -> Doc<'a> {
    unbreakable(text(format!("<{}>", run.url())))
}

#[cfg(test)]
mod tests {
    use super::{EscapedUrl, escape_bare_url, escape_title, escape_url};

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
