//! Inline-content serializer: `NodeKind::*` → `Doc<'a>`.
//!
//! Covers `Text`, `Code`, `Emphasis`, `Strong`, `Strikethrough`,
//! `Link`, `Image`, `Autolink`, `InlineHtml`, `FootnoteReference`,
//! `SoftBreak`, `HardBreak`. Reference-link definition collection
//! and end-of-document emission live in session 09; the wrap pass
//! that decides flatten-vs-break for [`Doc::Line`] lives in
//! session 10.
//!
//! The escape policy lives in [`super::escape`]; emphasis delimiter
//! normalisation comes from [`crate::config::FmtOptions::resolve_italic`].

use std::borrow::Cow;

use crate::format::ctx::Ctx;
use crate::format::doc::{Doc, concat, hard_line, line, text, unbreakable};
use crate::format::escape::{EscapeScope, escape_text, needs_escape_at};
use crate::tree::{LinkKind, NodeId, NodeKind};

/// Concatenate the inline children of `parent` using the default
/// escape scope. Convenience for block paragraphs / headings.
pub(crate) fn render_inline<'a>(ctx: &Ctx<'a>, parent: NodeId) -> Doc<'a> {
    render_inline_in_scope(ctx, parent, EscapeScope::default())
}

/// Same as [`render_inline`] but with an explicit scope (used by
/// table cells to set `in_table_cell = true`, by headings to set
/// `in_heading = true`, and recursively by link text to set
/// `in_link_text = true`).
pub(crate) fn render_inline_in_scope<'a>(
    ctx: &Ctx<'a>,
    parent: NodeId,
    scope: EscapeScope,
) -> Doc<'a> {
    let ids: Vec<NodeId> = ctx.tree.children(parent).collect();
    render_inline_nodes(ctx, &ids, scope)
}

/// Pending fragment in an inline text run. Adjacent text and the
/// line breaks between them all participate in `CommonMark`'s
/// emphasis-flanking decision (CM §6.2 treats line endings within
/// a paragraph as whitespace), so the run accumulates across
/// `SoftBreak` / `HardBreak` until a non-text inline node forces a
/// flush. The emission step then splits the escaped string back
/// at the recorded break positions.
enum Chunk<'a> {
    /// A text fragment from pulldown plus the source slice the fragment
    /// was derived from. The source slice is used to detect bytes that
    /// were originally backslash-escaped in the source (`\_`, `\*`, …):
    /// pulldown consumes the backslash and emits the bare byte, but CM
    /// §6.2 requires those bytes to *not* participate in emphasis
    /// matching. Re-emitting the bare byte lets pulldown re-parse them
    /// as fresh delimiter candidates, which can create `<em>` / `<strong>`
    /// spans the source never had. The source slice is `None` for
    /// fragments synthesised by the formatter (e.g., footnote-reference
    /// expansion) where no source escape is implied.
    Text {
        payload: Cow<'a, str>,
        source: Option<&'a str>,
    },
    SoftBreak,
    HardBreak,
}

/// True when the source placed this inline-HTML node at the start of
/// its source line with at least 4 columns of leading whitespace —
/// the canonical CM §4.6 paragraph-continuation pattern for a comment.
fn comment_indented_on_own_line_in_source(ctx: &Ctx<'_>, id: NodeId) -> bool {
    let Some(node) = ctx.tree.node(id) else {
        return false;
    };
    let start = node.raw_range.start;
    if start == 0 {
        return false;
    }
    let prefix = ctx.source.get(..start).unwrap_or("");
    let Some(nl) = prefix.rfind('\n') else {
        return false;
    };
    let Some(line_lead) = prefix.get(nl.saturating_add(1)..) else {
        return false;
    };
    line_lead.len() >= 4 && line_lead.bytes().all(|b| matches!(b, b' ' | b'\t'))
}

/// Render an arbitrary slice of sibling inline nodes. Used for list
/// items whose children mix inline leaves with no enclosing Paragraph.
///
/// Adjacent `Text` siblings — and any `SoftBreak` / `HardBreak` between
/// them — are coalesced before the escape policy runs. Pulldown
/// often splits a logical run at every backslash escape (`a \*b\*`
/// becomes five `Event::Text`s) and at every soft break, but CM's
/// emphasis matching ignores those splits and treats the whole
/// paragraph as one flanking context. Flushing on break would
/// hide partner asterisks across a `\n` from one another and let
/// the formatter emit pairs that pulldown then re-parses as `<em>`.
pub(crate) fn render_inline_nodes<'a>(
    ctx: &Ctx<'a>,
    ids: &[NodeId],
    scope: EscapeScope,
) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::with_capacity(ids.len());
    let mut text_run: Vec<Chunk<'a>> = Vec::new();
    let flush_text = |run: &mut Vec<Chunk<'a>>, parts: &mut Vec<Doc<'a>>| {
        flush_run(run, parts, scope);
    };
    // The resolved emphasis-delimiter of the most recent sibling
    // Emphasis (or `None` if the previous sibling is not Emphasis).
    // Used by `render_emphasis` to flip a `_↔*` collision against
    // an abutting emphasis sibling without an O(N²) parent walk.
    let mut prev_emphasis_delim: Option<u8> = None;
    for &cid in ids {
        let Some(node) = ctx.tree.node(cid) else {
            continue;
        };
        match &node.kind {
            NodeKind::Text(s) => {
                let src = ctx.tree.node(cid).map(|n| {
                    let r = &n.raw_range;
                    ctx.source.get(r.start..r.end).unwrap_or("")
                });
                text_run.push(Chunk::Text {
                    payload: s.clone(),
                    source: src,
                });
            }
            NodeKind::Code(s) => {
                flush_text(&mut text_run, &mut parts);
                parts.push(render_code(s.as_ref(), scope));
            }
            NodeKind::Emphasis => {
                flush_text(&mut text_run, &mut parts);
                let (doc, delim) = render_emphasis(ctx, cid, scope, prev_emphasis_delim);
                parts.push(doc);
                prev_emphasis_delim = Some(delim);
                continue;
            }
            NodeKind::Strong => {
                flush_text(&mut text_run, &mut parts);
                parts.push(render_strong(ctx, cid, scope));
            }
            NodeKind::Strikethrough => {
                flush_text(&mut text_run, &mut parts);
                parts.push(render_strikethrough(ctx, cid, scope));
            }
            NodeKind::Link { .. } => {
                flush_text(&mut text_run, &mut parts);
                parts.push(render_link(ctx, cid, scope));
            }
            NodeKind::Image { .. } => {
                flush_text(&mut text_run, &mut parts);
                parts.push(render_image(ctx, cid, scope));
            }
            NodeKind::Autolink { url, kind } => {
                flush_text(&mut text_run, &mut parts);
                parts.push(render_autolink(url.as_ref(), *kind));
            }
            NodeKind::InlineHtml(raw) => {
                flush_text(&mut text_run, &mut parts);
                // CM §4.6 type-2: a paragraph line whose first
                // non-space character is `<!--` within columns 1–3
                // starts an HTML block, ending the paragraph. With 4+
                // leading spaces the line stays paragraph continuation
                // and the `<!-- … -->` is inline. So when the source
                // placed an inline-HTML comment on its own line with
                // ≥4 spaces of indent, the formatted output must do
                // the same — otherwise wrap promotes the preceding
                // soft break to a hard break, the comment lands at
                // column 0, and pulldown re-parses it as a block,
                // splitting one paragraph into two.
                let comment_on_own_line =
                    raw.starts_with("<!--") && comment_indented_on_own_line_in_source(ctx, cid);
                let raw_str: Cow<'a, str> = if comment_on_own_line {
                    let mut joined = String::with_capacity(raw.len().saturating_add(4));
                    joined.push_str("    ");
                    joined.push_str(raw.as_ref());
                    Cow::Owned(joined)
                } else {
                    raw.clone()
                };
                parts.push(unbreakable(text(raw_str)));
            }
            NodeKind::FootnoteReference(label) => {
                flush_text(&mut text_run, &mut parts);
                parts.push(text(format!("[^{label}]")));
            }
            NodeKind::SoftBreak => text_run.push(Chunk::SoftBreak),
            NodeKind::HardBreak => text_run.push(Chunk::HardBreak),
            NodeKind::TaskListMarker(_) => {
                // The list-item renderer prepends `[x] ` / `[ ] `; skip
                // the leaf so we don't emit it twice.
            }
            // Structural kinds (Document, Paragraph, …) and the
            // `Unknown` forward-compat fallback should never appear
            // as inline children. Falling back to verbatim source
            // keeps the formatter robust against future pulldown
            // additions; the debug_assert flags the bug in tests.
            NodeKind::Document
            | NodeKind::Paragraph
            | NodeKind::Heading { .. }
            | NodeKind::BlockQuote
            | NodeKind::List { .. }
            | NodeKind::Item { .. }
            | NodeKind::CodeBlock { .. }
            | NodeKind::HtmlBlock
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
                flush_text(&mut text_run, &mut parts);
                // raw_text returns &'a str borrowed from source;
                // pass through as Cow::Borrowed.
                parts.push(text(ctx.tree.raw_text(cid)));
            }
        }
        // Reset the adjacent-emphasis tracker on every non-Emphasis
        // child; the Emphasis arm already `continue`d past this.
        prev_emphasis_delim = None;
    }
    flush_text(&mut text_run, &mut parts);
    concat(parts)
}

/// Apply the escape policy to a singleton text fragment, preserving
/// the original Cow's borrow when the escape policy is a no-op (the
/// overwhelming common case for plain prose). When `source` is given,
/// any payload byte that was backslash-escaped in source is forced
/// through the escape gate so CM-emphasis matching keeps treating it
/// as text on re-parse (see [`Chunk::Text`] for the full motivation).
fn render_text_cow<'a>(cow: Cow<'a, str>, source: Option<&str>, scope: EscapeScope) -> Doc<'a> {
    if let Some(src) = source
        && payload_has_source_escape(cow.as_ref(), src)
    {
        let out = escape_with_source(cow.as_ref(), src, scope);
        return text(out);
    }
    match escape_text(cow.as_ref(), scope) {
        Cow::Borrowed(_) => text(cow),
        Cow::Owned(s) => text(s),
    }
}

/// True iff `source` contains at least one CM `§2.4` backslash escape
/// (`\X` for some CM-punct byte `X`). Fast path: if false, the chunk
/// has no source-escape semantics to preserve and the plain
/// [`escape_text`] policy is enough.
fn payload_has_source_escape(payload: &str, source: &str) -> bool {
    source.len() > payload.len() && source.contains('\\')
}

/// Combined escape pass that honours both the standard CM-byte rule
/// (via [`needs_escape_at`]) and per-byte "forced escape" decisions
/// derived from `source`: any payload byte that was backslash-escaped
/// in source must remain so on output, or CM's emphasis matcher will
/// reintroduce delimiters that the source explicitly suppressed.
fn escape_with_source(payload: &str, source: &str, scope: EscapeScope) -> String {
    let forced = forced_escapes_from_source(payload, source);
    escape_combined(payload, &forced, scope)
}

/// Walk `source` (the raw slice the chunk's `payload` was derived from)
/// and return a bitmap, one entry per payload byte, marking which
/// payload bytes came from a `\X` escape in source. The mapping
/// follows CM §2.4: each `\X` (where `X` is CM punctuation) yields one
/// payload byte `X` consumed from two source bytes; every other source
/// byte maps 1:1 to the payload.
fn forced_escapes_from_source(payload: &str, source: &str) -> Vec<bool> {
    let mut forced = vec![false; payload.len()];
    let s = source.as_bytes();
    let p = payload.as_bytes();
    let mut si = 0usize;
    let mut pi = 0usize;
    while si < s.len() && pi < p.len() {
        let sb = s.get(si).copied();
        let pb = p.get(pi).copied();
        if sb == Some(b'\\')
            && si.saturating_add(1) < s.len()
            && let Some(next) = s.get(si.saturating_add(1)).copied()
            && pb == Some(next)
        {
            if let Some(slot) = forced.get_mut(pi) {
                *slot = true;
            }
            si = si.saturating_add(2);
            pi = pi.saturating_add(1);
        } else if sb == pb {
            si = si.saturating_add(1);
            pi = pi.saturating_add(1);
        } else {
            // Mismatch — pulldown decoded a non-trivial source span
            // (e.g., a CommonMark entity reference like `&amp;`).
            // Bail out conservatively: nothing forced from here on.
            break;
        }
    }
    forced
}

/// Emit a pending inline text run as Doc nodes. Soft breaks and hard
/// breaks inside the run participate in the escape policy's flanking
/// context (they're whitespace per CM §6.2) but emit as their
/// respective Doc primitives on output. A singleton text chunk takes
/// the zero-allocation `render_text_cow` path. Any byte that was
/// backslash-escaped in source is force-escaped on output, regardless
/// of what flanking context the surrounding chunks provide.
fn flush_run<'a>(run: &mut Vec<Chunk<'a>>, parts: &mut Vec<Doc<'a>>, scope: EscapeScope) {
    if run.is_empty() {
        return;
    }
    if run.len() == 1 {
        match run.pop() {
            Some(Chunk::Text { payload, source }) => {
                parts.push(render_text_cow(payload, source, scope));
                return;
            }
            Some(Chunk::SoftBreak) => {
                parts.push(line());
                return;
            }
            Some(Chunk::HardBreak) => {
                parts.push(render_hard_break(scope));
                return;
            }
            None => return,
        }
    }
    // Multi-chunk run: build one buffer with `\n` placeholders at
    // each break position, then a parallel "forced-escape" bitmap
    // recording which payload bytes were backslash-escaped in source.
    // A single combined pass handles both the standard CM-byte rule
    // and the forced-escape rule; segment boundaries (`\n`) round-trip
    // 1:1 because escape only inserts before existing bytes.
    let total: usize = run
        .iter()
        .map(|c| match c {
            Chunk::Text { payload, .. } => payload.len(),
            Chunk::SoftBreak | Chunk::HardBreak => 1,
        })
        .sum();
    let mut buf = String::with_capacity(total);
    let mut forced: Vec<bool> = Vec::with_capacity(total);
    let mut break_kinds: Vec<Chunk<'static>> = Vec::with_capacity(run.len());
    for chunk in run.drain(..) {
        match chunk {
            Chunk::Text { payload, source } => {
                let chunk_forced = if let Some(src) = source {
                    if payload_has_source_escape(payload.as_ref(), src) {
                        forced_escapes_from_source(payload.as_ref(), src)
                    } else {
                        vec![false; payload.len()]
                    }
                } else {
                    vec![false; payload.len()]
                };
                buf.push_str(payload.as_ref());
                forced.extend(chunk_forced);
            }
            Chunk::SoftBreak => {
                buf.push('\n');
                forced.push(false);
                break_kinds.push(Chunk::SoftBreak);
            }
            Chunk::HardBreak => {
                buf.push('\n');
                forced.push(false);
                break_kinds.push(Chunk::HardBreak);
            }
        }
    }
    let escaped = escape_combined(&buf, &forced, scope);
    let mut segments = escaped.split('\n');
    if let Some(first) = segments.next()
        && !first.is_empty()
    {
        parts.push(text(first.to_owned()));
    }
    for (i, seg) in segments.enumerate() {
        let bk = break_kinds.get(i);
        match bk {
            Some(Chunk::HardBreak) => parts.push(render_hard_break(scope)),
            _ => parts.push(line()),
        }
        if !seg.is_empty() {
            parts.push(text(seg.to_owned()));
        }
    }
}

/// Standard-escape pass combined with forced-escape bitmap. Mirrors
/// the byte loop inside [`escape_text`] but consults `forced` first.
/// Writes raw bytes into a `Vec<u8>` so multi-byte UTF-8 sequences
/// pass through verbatim — `String::push(char::from(byte))` would
/// re-encode every high byte as a Latin-1 codepoint and corrupt the
/// source's UTF-8.
fn escape_combined(buf: &str, forced: &[bool], scope: EscapeScope) -> String {
    let bytes = buf.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(buf.len().saturating_add(8));
    for i in 0..bytes.len() {
        let need = forced.get(i).copied().unwrap_or(false) || needs_escape_at(bytes, i, scope);
        if need {
            out.push(b'\\');
        }
        if let Some(b) = bytes.get(i).copied() {
            out.push(b);
        }
    }
    // Safe: every inserted byte is `\` (ASCII); source bytes are
    // copied 1:1 from a valid `&str`. UTF-8 boundaries preserved.
    String::from_utf8(out).unwrap_or_default()
}

/// CM §6.3 inline code span. Fence length is one more than the
/// longest backtick run in `content`; if `content` starts or ends
/// with a backtick or whitespace, pad with one space on each side.
/// Inside a table cell, GFM treats `\|` as a literal pipe even
/// within code-span backticks (cmark-gfm §6.11), so the renderer
/// escapes `|` accordingly.
fn render_code<'a>(content: &str, scope: EscapeScope) -> Doc<'a> {
    let longest = longest_backtick_run(content);
    let fence_len = longest.saturating_add(1);
    let needs_pad = content.starts_with('`')
        || content.ends_with('`')
        || content.starts_with(' ')
        || content.ends_with(' ');
    // Build the rendered span in one allocation (fence + optional
    // pad + body + pad + fence). Before: 2-4 separate allocations
    // (fence String, body_owned, optional replace(), final format!).
    let pad = usize::from(needs_pad);
    let escape_pipe = scope.in_table_cell && content.contains('|');
    let extra = if escape_pipe {
        content.bytes().filter(|&b| b == b'|').count()
    } else {
        0
    };
    let cap = content
        .len()
        .saturating_add(fence_len.saturating_mul(2))
        .saturating_add(pad.saturating_mul(2))
        .saturating_add(extra);
    let mut out = String::with_capacity(cap);
    for _ in 0..fence_len {
        out.push('`');
    }
    if needs_pad {
        out.push(' ');
    }
    if escape_pipe {
        // `|` is ASCII, so byte indices into `content` are valid
        // char boundaries. Walk by `|` separator and push slices
        // between hits — preserves multi-byte UTF-8.
        let mut last = 0usize;
        for (i, b) in content.bytes().enumerate() {
            if b == b'|' {
                out.push_str(content.get(last..i).unwrap_or(""));
                out.push_str("\\|");
                last = i.saturating_add(1);
            }
        }
        out.push_str(content.get(last..).unwrap_or(""));
    } else {
        out.push_str(content);
    }
    if needs_pad {
        out.push(' ');
    }
    for _ in 0..fence_len {
        out.push('`');
    }
    unbreakable(text(out))
}

fn longest_backtick_run(s: &str) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for b in s.bytes() {
        if b == b'`' {
            current = current.saturating_add(1);
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
        }
    }
    longest
}

fn render_emphasis<'a>(
    ctx: &Ctx<'a>,
    id: NodeId,
    scope: EscapeScope,
    prev_sibling_emphasis_delim: Option<u8>,
) -> (Doc<'a>, u8) {
    let source_delim = source_emphasis_delim(ctx, id);
    let mut delim = ctx.opts.resolve_italic(source_delim);
    // Avoid the `***foo` (or `___foo`) open-side merge case: when
    // the first child of this emphasis is Strong with the same
    // delimiter byte, the open emphasis delimiter would fuse with
    // the strong's `**`/`__` into a length-3 delimiter run; CM rule
    // 9 then re-pairs the run and may flip the strong/em nesting on
    // round-trip. Switch this emphasis to the other delimiter so the
    // open side stays unambiguous. Trailing-side adjacency is benign:
    // a `***`-run at end has a single matching open `***`, and rule 9
    // settles the split as (open `**`+`*`, close `*+**`).
    if first_child_is_strong(ctx, id) {
        delim = if delim == b'_' { b'*' } else { b'_' };
    }
    // CM §6.2 rule 9: two emphasis siblings that abut with no
    // intervening byte share a delimiter run on round-trip. So
    // `<em>a</em><em>b</em>` written naïvely as `*a**b*` re-pairs
    // as a literal `**` plus stray `*`s, and source like
    // `…R¹f*_G(T)…` (where pulldown emitted adjacent emphases with
    // different source delimiters) collapses when both are
    // normalised to the same byte. Flip this emphasis's delimiter
    // when the previous sibling is also an Emphasis whose resolved
    // delimiter would collide. The hint is threaded in from the
    // sibling walk in `render_inline_nodes`, avoiding an O(N²)
    // parent-children scan on emphasis-heavy paragraphs.
    if prev_sibling_emphasis_delim == Some(delim) {
        delim = if delim == b'_' { b'*' } else { b'_' };
    }
    let d: &'static str = if delim == b'_' { "_" } else { "*" };
    let inner = render_inline_in_scope(ctx, id, scope);
    (concat([text(d), inner, text(d)]), delim)
}

fn first_child_is_strong(ctx: &Ctx<'_>, id: NodeId) -> bool {
    ctx.tree
        .children(id)
        .next()
        .and_then(|i| ctx.tree.node(i))
        .is_some_and(|n| matches!(n.kind, NodeKind::Strong))
}

fn render_strong<'a>(ctx: &Ctx<'a>, id: NodeId, scope: EscapeScope) -> Doc<'a> {
    let source_delim = source_emphasis_delim(ctx, id);
    let mut delim = ctx.opts.resolve_italic(source_delim);
    // Symmetric to [`render_emphasis`]: if the first child is an
    // Emphasis, the strong's open `**`/`__` would fuse with the
    // emphasis's `*`/`_` into a length-3 run, and CM rule 9 may
    // re-pair it on round-trip. Trailing-side adjacency is benign.
    if first_child_is_emphasis(ctx, id) {
        delim = if delim == b'_' { b'*' } else { b'_' };
    }
    let d: &'static str = if delim == b'_' { "__" } else { "**" };
    let inner = render_inline_in_scope(ctx, id, scope);
    concat([text(d), inner, text(d)])
}

fn first_child_is_emphasis(ctx: &Ctx<'_>, id: NodeId) -> bool {
    ctx.tree
        .children(id)
        .next()
        .and_then(|i| ctx.tree.node(i))
        .is_some_and(|n| matches!(n.kind, NodeKind::Emphasis))
}

fn render_strikethrough<'a>(ctx: &Ctx<'a>, id: NodeId, scope: EscapeScope) -> Doc<'a> {
    let inner = render_inline_in_scope(ctx, id, scope);
    concat([text("~~"), inner, text("~~")])
}

/// Read the first byte of the node's raw source range. For an
/// Emphasis / Strong node this is the opening delimiter byte
/// (`b'*'` or `b'_'`). Falls back to `b'*'` if the range is empty
/// or starts with an unexpected byte; the latter happens when the
/// node lacks a source range (e.g., synthesized nodes), at which
/// point asterisk is the safer default.
fn source_emphasis_delim(ctx: &Ctx<'_>, id: NodeId) -> u8 {
    let raw = ctx.tree.raw_text(id);
    raw.bytes()
        .find(|b| matches!(b, b'*' | b'_'))
        .unwrap_or(b'*')
}

// ============================================================
// Links and images
// ============================================================

/// Value object shared between [`render_link`] and [`render_image`].
/// A record, not an abstraction — every field is borrowed from the
/// `NodeKind::Link` / `NodeKind::Image` payload.
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
            | NodeKind::HtmlBlock
            | NodeKind::ThematicBreak
            | NodeKind::Table { .. }
            | NodeKind::TableHead
            | NodeKind::TableRow
            | NodeKind::TableCell
            | NodeKind::FootnoteDefinition { .. }
            | NodeKind::LinkReferenceDefinition { .. }
            | NodeKind::Text(_)
            | NodeKind::Code(_)
            | NodeKind::Emphasis
            | NodeKind::Strong
            | NodeKind::Strikethrough
            | NodeKind::Autolink { .. }
            | NodeKind::InlineHtml(_)
            | NodeKind::FootnoteReference(_)
            | NodeKind::SoftBreak
            | NodeKind::HardBreak
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

fn render_link<'a>(ctx: &Ctx<'a>, id: NodeId, outer_scope: EscapeScope) -> Doc<'a> {
    render_link_or_image(ctx, id, outer_scope, false)
}

fn render_image<'a>(ctx: &Ctx<'a>, id: NodeId, outer_scope: EscapeScope) -> Doc<'a> {
    render_link_or_image(ctx, id, outer_scope, true)
}

fn render_link_or_image<'a>(
    ctx: &Ctx<'a>,
    id: NodeId,
    outer_scope: EscapeScope,
    is_image: bool,
) -> Doc<'a> {
    let Some(node) = ctx.tree.node(id) else {
        return concat([]);
    };
    let Some(target) = LinkTarget::from_node(&node.kind) else {
        return text(ctx.tree.raw_text(id));
    };

    // Inner-text scope: brackets must be escaped; preserve the outer
    // table-cell and heading flags so a link inside a table cell still
    // escapes pipes, and a hard break inside a link inside a heading
    // still emits `<br/>`.
    let text_scope = EscapeScope {
        in_link_text: true,
        in_table_cell: outer_scope.in_table_cell,
        in_heading: outer_scope.in_heading,
    };
    let text_doc = render_inline_in_scope(ctx, id, text_scope);

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
/// runs to a single ASCII space, then ASCII-lowercase. Full Unicode
/// case folding would require an extra dependency; the gentle-sga/i
/// corpus's labels are ASCII, and `to_lowercase` is a safe upgrade
/// later.
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
/// `style == Angle` forces angle form even when the URL would parse
/// bare.
fn render_url_destination(url: &str, style: crate::config::LinkDefStyle) -> Doc<'_> {
    if matches!(style, crate::config::LinkDefStyle::Angle) {
        return text(format!("<{}>", escape_angle_url(url)));
    }
    match escape_url(url) {
        // Preserve the Cow: bare URLs are Cow::Borrowed on the
        // common path (no unbalanced parens), so this avoids an
        // allocation per inline link.
        EscapedUrl::Bare(s) => text(s),
        EscapedUrl::Angle(s) => text(format!("<{s}>")),
    }
}

/// Output of [`escape_url`]: either a bare destination or one that
/// must be wrapped in `<…>`.
enum EscapedUrl<'a> {
    Bare(Cow<'a, str>),
    Angle(Cow<'a, str>),
}

/// Escape a URL for the `(dest)` portion of an inline link.
///
/// CM §6.3 destination forms:
/// - bare: any non-whitespace, non-control bytes; parentheses must
///   either balance or be backslash-escaped.
/// - angle: `<…>` — any byte except `<`, `>`, `\n`. Whitespace is
///   permitted here.
///
/// Strategy: prefer bare; switch to angle if the URL contains
/// whitespace. Inside the bare form, backslash-escape unbalanced
/// parens.
fn escape_url(url: &str) -> EscapedUrl<'_> {
    if url
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
    {
        // Angle form. Within `<…>`, backslash-escape `<`, `>`, and `\`.
        return EscapedUrl::Angle(escape_angle_url(url));
    }
    EscapedUrl::Bare(escape_bare_url(url))
}

/// In bare-URL form, escape `(`/`)` that don't balance with a partner
/// in the same string. CM permits balanced unescaped parens (the
/// common case for Wikipedia-style URLs).
fn escape_bare_url(url: &str) -> Cow<'_, str> {
    let bytes = url.as_bytes();
    // Two-pass: build a bitmap of paren indices that need escape.
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

/// Inside `<…>`, escape `<`, `>`, and `\`. The CM tokenizer also
/// rejects literal `\n`, but if pulldown produced one we emit it
/// escaped rather than dropping it.
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

/// Escape a link title for emission inside double quotes: `\` and
/// `"` get a leading backslash; all other bytes pass through. Pure;
/// `Cow::Borrowed` on the common path.
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
// Autolinks, breaks, inline HTML, footnote refs
// ============================================================

fn render_autolink<'a>(url: &str, _kind: crate::tree::AutolinkKind) -> Doc<'a> {
    // URI and Email autolinks share the same `<…>` form on output;
    // the IR's `AutolinkKind` is preserved for future linter rules.
    unbreakable(text(format!("<{url}>")))
}

fn render_hard_break<'a>(scope: EscapeScope) -> Doc<'a> {
    if scope.in_heading {
        // CommonMark: hard breaks inside a heading render to <br/>;
        // the surrounding heading is single-line, so we emit the tag
        // literally rather than break the line.
        text("<br/>")
    } else {
        // Backslash form (CM §6.7) chosen over `"  \n"` so the
        // trailing-whitespace linter doesn't fight the formatter.
        concat([text("\\"), hard_line()])
    }
}

// ============================================================
// Tests
// ============================================================

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
