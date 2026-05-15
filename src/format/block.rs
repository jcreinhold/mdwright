//! Block-level Markdown serializer: `Tree<'a>` → `Doc<'a>`.
//!
//! Inline content (the children of [`NodeKind::Paragraph`],
//! [`NodeKind::Heading`], [`NodeKind::Item`], …) is delegated to
//! [`super::inline::render_inline`], which in this session is a
//! source-verbatim stub. Block kinds choose their own indent and
//! line-break discipline; soft wrapping is deferred to a later
//! session and so every line break here is a [`Doc::HardLine`].

use std::collections::HashSet;
use std::ops::Range;

use crate::config::{LinkDefStyle, OrderedListStyle, Placement, Wrap};
use crate::format::ctx::Ctx;
use crate::format::doc::{Doc, RenderOptions, concat, hard_line, render, text, unbreakable};
use crate::format::inline::{render_inline, render_inline_nodes};
use crate::format::verbatim::emit_verbatim;
use crate::format::wrap::wrap_doc;
use crate::tree::{NodeId, NodeKind, TableAlign};

// ============================================================
// Public dispatch
// ============================================================

/// Render every direct block child of `parent` separated by a blank
/// line. Block helpers emit a trailing `HardLine`, so two consecutive
/// blocks produce one blank line between them; this routine inserts
/// the *second* hard line.
/// True iff the byte range `block` overlaps any math region. Used by
/// [`render_block_sequence`] to short-circuit IR-driven emission for
/// blocks containing `\[ … \]` / `\( … \)` content — those are
/// emitted byte-verbatim from `ctx.source` so pulldown's re-parse
/// matches the source's parse and the HTML gate stays green.
fn block_overlaps_math(ctx: &Ctx<'_>, block: &Range<usize>) -> bool {
    ctx.math_regions
        .iter()
        .any(|r| r.range.start < block.end && block.start < r.range.end)
}

pub(crate) fn render_block_sequence<'a>(ctx: &Ctx<'a>, parent: NodeId) -> Doc<'a> {
    let is_doc_root = parent == ctx.tree.root();
    let end_placement = ctx.opts.link_def_placement() == Placement::End;
    let footnote_end = ctx.opts.footnote_placement() == Placement::End;
    let mut parts: Vec<Doc<'a>> = Vec::new();
    let mut emitted = 0usize;

    // Frontmatter: emit verbatim at the very top of the document.
    if is_doc_root
        && ctx.opts.preserve_frontmatter()
        && let Some(fm) = ctx.frontmatter
    {
        parts.push(unbreakable(verbatim_lines(fm.slice.text)));
        emitted = emitted.saturating_add(1);
    }

    let mut adm_idx = 0usize;
    let mut emitted_adm: Option<usize> = None;
    for child in ctx.tree.children(parent) {
        // Under End placement at the document root, skip
        // LinkReferenceDefinition nodes so the tail pass can sort
        // and emit them in one place — without leaving stray
        // separator newlines from the inter-block hard_line below.
        if is_doc_root
            && end_placement
            && matches!(
                ctx.tree.node(child).map(|n| &n.kind),
                Some(NodeKind::LinkReferenceDefinition { .. })
            )
        {
            continue;
        }
        // Under End placement at the document root, defer
        // FootnoteDefinition nodes to a sorted tail pass.
        if is_doc_root
            && footnote_end
            && matches!(
                ctx.tree.node(child).map(|n| &n.kind),
                Some(NodeKind::FootnoteDefinition { .. })
            )
        {
            continue;
        }
        // Admonition: every child whose raw_range falls inside an
        // admonition region is replaced by the region's verbatim
        // text, emitted exactly once.
        if let Some(node) = ctx.tree.node(child) {
            let cr = node.raw_range.clone();
            while adm_idx < ctx.admonitions.len()
                && ctx.admonitions.get(adm_idx).map_or(0, |a| a.range.end) <= cr.start
            {
                adm_idx = adm_idx.saturating_add(1);
            }
            if let Some(region) = ctx.admonitions.get(adm_idx)
                && cr.start >= region.range.start
                && cr.start < region.range.end
            {
                if emitted_adm != Some(adm_idx) {
                    if emitted > 0 {
                        parts.push(hard_line());
                    }
                    parts.push(unbreakable(verbatim_lines(region.text)));
                    emitted = emitted.saturating_add(1);
                    emitted_adm = Some(adm_idx);
                }
                continue;
            }
        }
        // Math overlay: any block whose source range overlaps a math
        // region is emitted byte-verbatim. CM tokenisation inside
        // `\[ … \]` is brittle (subscripts read as emphasis, etc.);
        // emitting the surrounding block verbatim guarantees
        // pulldown sees the same bytes on re-parse, so the HTML
        // gate stays green by construction. The trade-off is that
        // math-containing blocks aren't re-wrapped — a deliberate
        // choice motivated by mdwright's "math-resilient" mandate.
        if let Some(node) = ctx.tree.node(child)
            && block_overlaps_math(ctx, &node.raw_range)
        {
            if emitted > 0 {
                parts.push(hard_line());
            }
            let raw = ctx.source.get(node.raw_range.clone()).unwrap_or("");
            parts.push(unbreakable(verbatim_lines(raw)));
            emitted = emitted.saturating_add(1);
            continue;
        }
        if emitted > 0 {
            parts.push(hard_line());
        }
        parts.push(render_block(ctx, child));
        emitted = emitted.saturating_add(1);
    }
    if is_doc_root && end_placement {
        append_link_def_tail(ctx, &mut parts);
    }
    if is_doc_root && footnote_end {
        append_footnote_def_tail(ctx, &mut parts);
    }
    concat(parts)
}

/// Build a `Doc` for `raw` that emits the input byte-verbatim with
/// a terminating newline. Returns a single `Doc::Text` (`Cow::Borrowed`
/// from the source slice) plus a terminating `HardLine`; the caller
/// wraps in `unbreakable` so the embedded newlines never enter a
/// wrap run. Before: one `to_owned()` per line and one `HardLine`
/// node per line, both proportional to the input height; see
/// `format/corpus/none-wrap` bench.
fn verbatim_lines(raw: &str) -> Doc<'_> {
    let trimmed = raw.trim_end_matches('\n');
    if trimmed.is_empty() {
        return hard_line();
    }
    concat([text(trimmed), hard_line()])
}

/// Decide whether a paragraph can round-trip through verbatim
/// emission without losing any normalisation.
///
/// Requirements: (a) every inline child is a single-text-segment
/// [`InlineRun`] (no soft/hard breaks, no structural inlines like
/// emphasis/code/links), so source-byte emission cannot drop a break
/// the IR would otherwise have flattened or rewrapped; (b) the wrap
/// policy is [`Wrap::Keep`] — both [`Wrap::No`] (collapse soft
/// breaks) and [`Wrap::At(_)`] (re-wrap) require an IR-driven pass.
fn paragraph_is_verbatim_eligible(ctx: &Ctx<'_>, id: NodeId) -> bool {
    if !matches!(ctx.opts.wrap(), Wrap::Keep) {
        return false;
    }
    for child in ctx.tree.children(id) {
        let Some(node) = ctx.tree.node(child) else {
            continue;
        };
        let NodeKind::Run(run) = &node.kind else {
            return false;
        };
        // A run with breaks must go through IR-driven emission so
        // wrap can decide line layout. A run with multiple text
        // segments (which would imply breaks between them) is also
        // disqualified for the same reason.
        use crate::cm::inline::run::RunPart;
        let mut text_count = 0usize;
        for part in run.parts() {
            match part {
                RunPart::Text(_) => {
                    text_count = text_count.saturating_add(1);
                    if text_count > 1 {
                        return false;
                    }
                }
                RunPart::SoftBreak | RunPart::HardLineBreak | RunPart::HardBreakTag => {
                    return false;
                }
            }
        }
    }
    true
}

/// At the document root under [`Placement::End`], emit a tail block
/// containing every footnote definition collected in the tree,
/// sorted case-insensitively by label (stable on ties).
fn append_footnote_def_tail<'a>(ctx: &Ctx<'a>, parts: &mut Vec<Doc<'a>>) {
    // Collect footnote definitions in *source order*. Pulldown's HTML
    // renderer emits the `<div class="footnote-definition">` blocks in
    // the order it sees them, with `id` attributes derived from the
    // label; sorting alphabetically here changes the HTML byte stream
    // even when the rendered footnote text is identical, which fails
    // `format_validated`. Source order keeps `render_html(source) ==
    // render_html(formatted)`.
    let mut defs: Vec<(NodeId, usize)> = Vec::new();
    for child in ctx.tree.children(ctx.tree.root()) {
        let Some(node) = ctx.tree.node(child) else {
            continue;
        };
        if matches!(node.kind, NodeKind::FootnoteDefinition { .. }) {
            defs.push((child, node.raw_range.start));
        }
    }
    if defs.is_empty() {
        return;
    }
    defs.sort_by_key(|d| d.1);
    if !parts.is_empty() {
        parts.push(hard_line());
    }
    for (i, (child, _)) in defs.iter().enumerate() {
        if i > 0 {
            parts.push(hard_line());
        }
        parts.push(render_block(ctx, *child));
    }
}

/// At the document root under [`Placement::End`], append a sorted,
/// deduplicated block of every link reference definition collected
/// from the document. Footnote-shaped labels (`^foo`) are excluded —
/// they belong to a separate emission path.
fn append_link_def_tail<'a>(ctx: &Ctx<'a>, parts: &mut Vec<Doc<'a>>) {
    let mut items: Vec<(String, NodeId, usize)> = Vec::new();
    for child in ctx.tree.children(ctx.tree.root()) {
        let Some(node) = ctx.tree.node(child) else {
            continue;
        };
        if let NodeKind::LinkReferenceDefinition { label, .. } = &node.kind
            && !label.starts_with('^')
        {
            items.push((label.to_ascii_lowercase(), child, node.raw_range.start));
        }
    }
    if items.is_empty() {
        return;
    }
    items.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)));
    let mut seen: HashSet<String> = HashSet::new();
    items.retain(|i| seen.insert(i.0.clone()));
    if !parts.is_empty() {
        parts.push(hard_line());
    }
    let style = ctx.opts.link_def_style();
    for (_, child, _) in items {
        if let Some(node) = ctx.tree.node(child)
            && let NodeKind::LinkReferenceDefinition { label, dest, title } = &node.kind
        {
            parts.push(render_link_ref_def(
                label.as_ref(),
                dest.as_ref(),
                title.as_deref(),
                style,
            ));
        }
    }
}

/// Dispatch on the kind of `id` and return its `Doc`. Each arm
/// delegates to a small free helper named for what it emits. For
/// unknown / inline-only kinds (which should not appear as a block
/// in a well-formed tree) we fall back to verbatim source emission.
pub(crate) fn render_block<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let Some(node) = ctx.tree.node(id) else {
        return concat([]);
    };
    // At the document root, route block kinds whose only divergence
    // from the source is pulldown's re-tokenisation through
    // `emit_verbatim`. Restricted to direct children of the root:
    // nested-container blocks have continuation prefixes (`>`, list
    // indent) embedded inside their `raw_range`, which would double-
    // emit under the surrounding blockquote/list serializer.
    if ctx.tree.parent(id) == Some(ctx.tree.root()) {
        #[allow(clippy::wildcard_enum_match_arm)]
        match &node.kind {
            NodeKind::HtmlBlock { .. } => return emit_verbatim(ctx.tree, id),
            NodeKind::CodeBlock { fenced: false, .. } => return emit_verbatim(ctx.tree, id),
            NodeKind::Paragraph if paragraph_is_verbatim_eligible(ctx, id) => {
                return emit_verbatim(ctx.tree, id);
            }
            _ => {}
        }
    }
    match &node.kind {
        NodeKind::Paragraph => render_paragraph(ctx, id),
        NodeKind::Heading { level, setext } => render_heading(ctx, id, *level, *setext),
        NodeKind::BlockQuote => render_blockquote(ctx, id),
        NodeKind::CodeBlock { fenced, info, .. } => {
            render_code_block(ctx, id, *fenced, info.as_ref())
        }
        NodeKind::HtmlBlock { .. } => render_html_block(ctx, id),
        NodeKind::ThematicBreak => render_thematic_break(),
        NodeKind::List {
            ordered,
            start,
            tight,
            marker_byte,
        } => render_list(ctx, id, *ordered, *start, *tight, *marker_byte),
        NodeKind::Table { alignments } => render_table(ctx, id, alignments),
        NodeKind::FootnoteDefinition { label } => render_footnote_def(ctx, id, label.as_ref()),
        NodeKind::LinkReferenceDefinition { label, dest, title } => {
            // The flat-IR's link-def scan also picks up footnote
            // definitions (`[^a]: …`) because they share the prefix
            // shape. Pulldown emits those separately as
            // `FootnoteDefinition`, so suppress the synthesised copy
            // here to avoid double emission. Under End placement the
            // tail pass owns emission; suppress here too.
            if label.starts_with('^') || ctx.opts.link_def_placement() == Placement::End {
                concat([])
            } else {
                render_link_ref_def(
                    label.as_ref(),
                    dest.as_ref(),
                    title.as_deref(),
                    ctx.opts.link_def_style(),
                )
            }
        }
        // Container kinds we do not expect as direct block children:
        // `Document` is the root (handled by `render_block_sequence`);
        // `Item` is handled by `render_list`; table sub-parts are
        // handled by `render_table`. Inline-only kinds should not
        // appear at block position in a well-formed tree. For all of
        // these we fall back to verbatim source emission.
        NodeKind::Document
        | NodeKind::Item { .. }
        | NodeKind::TableHead
        | NodeKind::TableRow
        | NodeKind::TableCell
        | NodeKind::Run(_)
        | NodeKind::CodeRun(_)
        | NodeKind::Emphasis
        | NodeKind::Strong
        | NodeKind::Strikethrough
        | NodeKind::Link { .. }
        | NodeKind::Image { .. }
        | NodeKind::Autolink { .. }
        | NodeKind::HtmlSpan(_)
        | NodeKind::FootnoteReference(_)
        | NodeKind::TaskListMarker(_)
        | NodeKind::Unknown { .. } => concat([text(ctx.tree.raw_text(id)), hard_line()]),
    }
}

// ============================================================
// Paragraph
// ============================================================

fn render_paragraph<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let inline = render_inline(ctx, id);
    let body = escape_paragraph_line_starts(ctx, inline);
    concat([body, hard_line()])
}

/// Walk a paragraph's `Doc` and prepend a backslash to any text run
/// that, taken as the start of a logical line, would open a *block*
/// construct (ATX heading, list item, blockquote, code fence,
/// thematic break, indented code block).
///
/// "Logical line" here means: the very first text fragment of the
/// paragraph, plus every text fragment immediately following a
/// `HardLine`. The inline stub doesn't emit `HardLine` for soft
/// breaks, so in practice this fires only on the paragraph's first
/// fragment — which is enough for the cases the linter cares about
/// (a paragraph that *starts* with `- ` would otherwise reparse as
/// a list).
fn escape_paragraph_line_starts<'a>(ctx: &Ctx<'a>, doc: Doc<'a>) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::new();
    flatten(doc, &mut parts);
    coalesce_adjacent_text(&mut parts);
    let mut at_line_start = true;
    let mut out: Vec<Doc<'a>> = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        match part {
            Doc::Text(s) => {
                if at_line_start
                    && !s.is_empty()
                    && let Some(escaped) =
                        escape_for_block_start(s.as_ref(), ctx.source, next_on_same_line(&parts, i))
                {
                    out.push(text(escaped));
                    at_line_start = false;
                    continue;
                }
                if !s.is_empty() {
                    at_line_start = false;
                }
            }
            Doc::HardLine => at_line_start = true,
            // Any non-text, non-HardLine node consumes line-start
            // status: a soft `Line`, a `Group`, etc. all count as
            // mid-line content for the next text fragment.
            Doc::Line | Doc::Concat(_) | Doc::Atomic(_) => {
                at_line_start = false;
            }
        }
        out.push(part.clone());
    }
    concat(out)
}

/// Peek at the next `Doc` after position `i` to decide what follows
/// a line-leading text fragment on the same logical line. Returns
/// `LineContext::EndOfLine` if the next sibling is a `HardLine` or
/// end-of-paragraph (a line break in the emitted output); otherwise
/// `LineContext::MoreContent`. Used by [`escape_for_block_start`] to
/// distinguish `*` opening emphasis (more content follows on the
/// same line — not a bullet marker) from `*` alone on a line (which
/// CM does parse as a bullet open).
fn next_on_same_line(parts: &[Doc<'_>], i: usize) -> LineContext {
    match parts.get(i.saturating_add(1)) {
        Some(Doc::HardLine) | None => LineContext::EndOfLine,
        Some(_) => LineContext::MoreContent,
    }
}

#[derive(Copy, Clone, Debug)]
enum LineContext {
    MoreContent,
    EndOfLine,
}

/// Merge runs of consecutive `Doc::Text` leaves into one. Pulldown
/// often splits a logical text run across multiple events (e.g.,
/// around a backslash escape `1\.` becomes `Text("1") Text(".")`),
/// and the line-start escape pass needs to see the whole prefix to
/// decide whether to escape. Other `Doc` constructors are left alone.
fn coalesce_adjacent_text<'a>(parts: &mut Vec<Doc<'a>>) {
    if parts.len() < 2 {
        return;
    }
    let drained: Vec<Doc<'a>> = std::mem::take(parts);
    let mut merged: Vec<Doc<'a>> = Vec::with_capacity(drained.len());
    for part in drained {
        match (merged.last_mut(), part) {
            (Some(Doc::Text(prev)), Doc::Text(next)) => {
                let mut joined = String::with_capacity(prev.len().saturating_add(next.len()));
                joined.push_str(prev.as_ref());
                joined.push_str(next.as_ref());
                *prev = std::borrow::Cow::Owned(joined);
            }
            (_, part) => merged.push(part),
        }
    }
    *parts = merged;
}

/// Flatten a `Concat` tree into its leaves; preserves order. Other
/// `Doc` constructors (`Group`, `Nest`, `IfFlat`) don't appear in
/// the stub inline output, so we treat them as opaque leaves.
fn flatten<'a>(doc: Doc<'a>, out: &mut Vec<Doc<'a>>) {
    match doc {
        Doc::Concat(items) => {
            for item in items.into_vec() {
                flatten(item, out);
            }
        }
        leaf @ (Doc::Text(_) | Doc::Line | Doc::HardLine | Doc::Atomic(_)) => {
            out.push(leaf);
        }
    }
}

/// If `s` begins with a byte that would open a block construct at
/// the start of a line, return `Some(escaped)` with a leading `\`.
/// Otherwise `None`. `next` describes what follows on the same
/// emitted line: an emphasis open delimiter is a single-byte `*`
/// text fragment with `LineContext::MoreContent` afterwards
/// (the inner content of the emphasis), and must not be escaped —
/// otherwise CM stops seeing it as a delimiter.
fn escape_for_block_start(s: &str, _source: &str, next: LineContext) -> Option<String> {
    let bytes = s.as_bytes();
    let first = *bytes.first()?;
    let two: Option<u8> = bytes.get(1).copied();
    // A single-byte text fragment with content following on the same
    // emitted line cannot be a bullet- or ordered-list marker (CM
    // requires the marker to be followed by whitespace within the
    // first line), so skip the escape decision entirely.
    let fragment_continues_inline = two.is_none() && matches!(next, LineContext::MoreContent);
    let needs_escape = match first {
        b'#' => true,
        b'>' => true,
        b'-' | b'+' | b'*' => {
            // List item / thematic break opener: only if followed by
            // a space, tab, or actual end-of-line (list marker), or
            // repeated three-plus times (thematic break). `**bold**`
            // and `--em-dash` style runs are not openers, nor is `*`
            // immediately followed by inline content on the same line
            // (e.g., the emphasis open delimiter `*` + code span).
            if fragment_continues_inline {
                false
            } else {
                matches!(two, Some(b' ' | b'\t') | None)
                    || (two == Some(first) && bytes.get(2).copied() == Some(first))
            }
        }
        b'=' => {
            // Setext underline only at start of *second* logical line
            // of a paragraph; conservatively escape `===` runs.
            two == Some(b'=')
        }
        b'`' | b'~' => {
            // Fence opener: ≥3 of the same byte.
            two == Some(first) && bytes.get(2).copied() == Some(first)
        }
        b'0'..=b'9' => {
            // Ordered list marker: digits, then `.` or `)`, then space.
            let mut i = 1usize;
            while i < bytes.len() && bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i = i.saturating_add(1);
            }
            let punct = bytes.get(i).copied();
            let after = bytes.get(i.saturating_add(1)).copied();
            // If the marker punct is itself in *this* text fragment,
            // we have enough to decide. If the fragment ends with the
            // digit run and inline content continues after, this is
            // not a list marker (no `.` / `)` reachable here).
            if punct.is_none() && matches!(next, LineContext::MoreContent) {
                false
            } else {
                matches!(punct, Some(b'.' | b')')) && matches!(after, Some(b' ' | b'\t') | None)
            }
        }
        b' ' if bytes.starts_with(b"    ") => true,
        _ => false,
    };
    if !needs_escape {
        return None;
    }
    let mut esc = String::with_capacity(s.len().saturating_add(2));
    // For ordered list markers we escape the `.` / `)` rather than
    // the leading digit, matching mdformat's convention; for the
    // others we escape the first byte.
    if first.is_ascii_digit() {
        let mut i = 0usize;
        while i < bytes.len() && bytes.get(i).is_some_and(u8::is_ascii_digit) {
            esc.push(char::from(*bytes.get(i)?));
            i = i.saturating_add(1);
        }
        esc.push('\\');
        if let Some(b) = bytes.get(i).copied() {
            esc.push(char::from(b));
            i = i.saturating_add(1);
        }
        esc.push_str(s.get(i..)?);
    } else {
        esc.push('\\');
        esc.push_str(s);
    }
    Some(esc)
}

// ============================================================
// Heading
// ============================================================

fn render_heading<'a>(ctx: &Ctx<'a>, id: NodeId, level: u32, setext: bool) -> Doc<'a> {
    let inline = render_inline(ctx, id);
    if setext && level <= 2 {
        // Setext underlines: H1 uses `=`, H2 uses `-`. Width matches
        // the inline content's display width, minimum 3.
        let rendered = render_to_string(&inline);
        let width = rendered
            .lines()
            .next()
            .map_or(3, |l| l.chars().count())
            .max(3);
        let underline_char = if level == 1 { '=' } else { '-' };
        let underline: String = std::iter::repeat_n(underline_char, width).collect();
        return concat([inline, hard_line(), text(underline), hard_line()]);
    }
    let lvl = level.clamp(1, 6) as usize;
    let prefix: String = std::iter::repeat_n('#', lvl).collect::<String>() + " ";
    concat([text(prefix), inline, hard_line()])
}

// ============================================================
// Code blocks
// ============================================================

fn render_code_block<'a>(ctx: &Ctx<'a>, id: NodeId, fenced: bool, info: &str) -> Doc<'a> {
    let body = code_block_body(ctx, id, fenced);
    let fence_char = if fenced {
        source_fence_char(ctx, id).unwrap_or('`')
    } else {
        '`'
    };
    // CM §4.5: the opening fence must contain at least one more
    // fence character than any run in the body, otherwise the body's
    // inner fence closes the outer block. Pick max(source_len, body+1, 3).
    let body_max_run = longest_fence_run(&body, fence_char);
    let source_len = source_fence_len(ctx, id, fence_char).unwrap_or(3);
    let fence_len = source_len.max(body_max_run.saturating_add(1)).max(3);
    let fence_string: String = std::iter::repeat_n(fence_char, fence_len).collect();
    let fence_str: &str = fence_string.as_str();
    let mut open = String::with_capacity(fence_str.len().saturating_add(info.len()));
    open.push_str(fence_str);
    open.push_str(info);
    if body.is_empty() {
        return concat([
            unbreakable(concat([
                text(open),
                hard_line(),
                text(fence_string.clone()),
            ])),
            hard_line(),
        ]);
    }
    // Splice body + closing fence into one Doc::Text payload.
    // Wrap the whole sequence in `unbreakable` so wrap.rs treats it
    // as one atomic box — code-block lines must keep their indent
    // even when the surrounding wrap is `At(n)`.
    let mut tail =
        String::with_capacity(body.len().saturating_add(fence_str.len()).saturating_add(1));
    tail.push_str(&body);
    if !tail.ends_with('\n') {
        tail.push('\n');
    }
    tail.push_str(fence_str);
    concat([
        unbreakable(concat([text(open), hard_line(), text(tail)])),
        hard_line(),
    ])
}

/// Extract the body of a code block. The builder already collected
/// the parser's text payloads onto the [`NodeKind::CodeBlock::body`]
/// field; this returns that body with trailing newlines trimmed.
fn code_block_body(ctx: &Ctx<'_>, id: NodeId, _fenced: bool) -> String {
    let body = match ctx.tree.node(id).map(|n| &n.kind) {
        Some(NodeKind::CodeBlock { body, .. }) => body.as_ref(),
        _ => "",
    };
    let mut out = body.to_owned();
    while out.ends_with('\n') {
        let _ = out.pop();
    }
    out
}

fn source_fence_char(ctx: &Ctx<'_>, id: NodeId) -> Option<char> {
    let raw = ctx.tree.raw_text(id);
    raw.bytes()
        .find(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        .map(char::from)
        .filter(|c| *c == '`' || *c == '~')
}

/// Length of the opening fence run in the source for this code block,
/// when it matches `fence_char`. Returns `None` for indented code blocks
/// or if the source could not be inspected.
fn source_fence_len(ctx: &Ctx<'_>, id: NodeId, fence_char: char) -> Option<usize> {
    let fc = fence_char as u8;
    let raw = ctx.tree.raw_text(id);
    let bytes = raw.as_bytes();
    let start = bytes
        .iter()
        .position(|b| !matches!(*b, b' ' | b'\t' | b'\n' | b'\r'))?;
    if bytes.get(start).copied() != Some(fc) {
        return None;
    }
    let mut i = start;
    while bytes.get(i).copied() == Some(fc) {
        i = i.saturating_add(1);
    }
    Some(i.saturating_sub(start))
}

/// Longest run of `fence_char` appearing on any line of `body`.
/// CM §4.5: the opening fence must be strictly longer than the
/// longest such run for the body to be emitted verbatim.
fn longest_fence_run(body: &str, fence_char: char) -> usize {
    let fc = fence_char as u8;
    let mut max_run = 0usize;
    for line in body.as_bytes().split(|b| *b == b'\n') {
        // Skip leading whitespace; fences only count when they are
        // the first non-space content on the line.
        let mut i = 0;
        while matches!(line.get(i).copied(), Some(b' ' | b'\t')) {
            i = i.saturating_add(1);
        }
        if line.get(i).copied() != Some(fc) {
            continue;
        }
        let start = i;
        while line.get(i).copied() == Some(fc) {
            i = i.saturating_add(1);
        }
        max_run = max_run.max(i.saturating_sub(start));
    }
    max_run
}

// ============================================================
// Blockquote
// ============================================================

fn render_blockquote<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let inner = render_block_sequence(ctx, id);
    // Each emitted line is prefixed with "> " (2 columns).
    let rendered = render_to_string_with(inner, ctx.opts.wrap().shrink(2));
    let mut prefixed = String::with_capacity(rendered.len().saturating_add(rendered.len() / 32));
    for (i, line) in rendered.split('\n').enumerate() {
        if i > 0 {
            prefixed.push('\n');
        }
        if line.is_empty() {
            prefixed.push('>');
        } else {
            prefixed.push_str("> ");
            prefixed.push_str(line);
        }
    }
    // Trim a trailing `> ` / `>` row that the inner's terminating
    // HardLine produced. The blockquote is unbreakable, so we emit
    // the whole prefixed buffer as a single Doc::Text containing
    // newlines — the renderer push_str's it as-is. Before: one
    // `to_owned()` and one HardLine per line.
    let trimmed = trim_trailing_blockquote_marker(&prefixed);
    concat([unbreakable(text(trimmed)), hard_line()])
}

fn trim_trailing_blockquote_marker(s: &str) -> String {
    let mut out = s.to_owned();
    while out.ends_with("\n>") || out.ends_with("\n> ") {
        if let Some(idx) = out.rfind('\n') {
            out.truncate(idx);
        } else {
            break;
        }
    }
    out
}

// ============================================================
// Thematic break, HTML block
// ============================================================

fn render_thematic_break<'a>() -> Doc<'a> {
    concat([text("---"), hard_line()])
}

fn render_html_block<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let body = match ctx.tree.node(id).map(|n| &n.kind) {
        Some(NodeKind::HtmlBlock { body }) => body.as_ref(),
        _ => "",
    };
    let trimmed = body.trim_end_matches('\n');
    if trimmed.is_empty() {
        return hard_line();
    }
    concat([text(trimmed.to_owned()), hard_line()])
}

// ============================================================
// Lists
// ============================================================

fn render_list<'a>(
    ctx: &Ctx<'a>,
    id: NodeId,
    ordered: bool,
    start: u64,
    tight: bool,
    marker_byte: u8,
) -> Doc<'a> {
    let mut parts: Vec<Doc<'a>> = Vec::new();
    let items: Vec<NodeId> = ctx.tree.children(id).collect();
    for (idx, item_id) in items.iter().copied().enumerate() {
        if idx > 0 {
            parts.push(hard_line());
            if !tight {
                parts.push(hard_line());
            }
        }
        let marker = marker_for_item(ctx, ordered, start, marker_byte, idx, item_id);
        parts.push(render_item(ctx, item_id, &marker));
    }
    // Honour the block-helper contract documented on
    // `render_block_sequence`: every block ends with one HardLine
    // so the sequence's inter-block separator produces a blank
    // line. Without this, a list followed by a paragraph emits
    // them adjacent and the next parse absorbs the paragraph into
    // the last item.
    parts.push(hard_line());
    concat(parts)
}

fn marker_for_item(
    ctx: &Ctx<'_>,
    ordered: bool,
    start: u64,
    marker_byte: u8,
    idx: usize,
    item_id: NodeId,
) -> String {
    if ordered {
        let n = match ctx.opts.ordered_list() {
            OrderedListStyle::Consistent => start.saturating_add(idx as u64),
            OrderedListStyle::Preserve => source_ordered_marker_number(ctx, item_id)
                .unwrap_or_else(|| start.saturating_add(idx as u64)),
        };
        let punct = source_ordered_punct(ctx, item_id).unwrap_or('.');
        format!("{n}{punct} ")
    } else {
        let b = ctx.opts.resolve_list_marker(marker_byte);
        format!("{} ", char::from(b))
    }
}

fn source_ordered_marker_number(ctx: &Ctx<'_>, item_id: NodeId) -> Option<u64> {
    let raw = ctx.tree.raw_text(item_id);
    let trimmed = raw.trim_start();
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn source_ordered_punct(ctx: &Ctx<'_>, item_id: NodeId) -> Option<char> {
    let raw = ctx.tree.raw_text(item_id);
    let trimmed = raw.trim_start();
    trimmed
        .chars()
        .find(|c| !c.is_ascii_digit())
        .filter(|c| *c == '.' || *c == ')')
}

fn render_item<'a>(ctx: &Ctx<'a>, id: NodeId, marker: &str) -> Doc<'a> {
    let item_task: Option<bool> = ctx.tree.node(id).and_then(|n| {
        if let NodeKind::Item { task } = &n.kind {
            *task
        } else {
            None
        }
    });
    let task_prefix = match item_task {
        Some(true) => Some("[x] "),
        Some(false) => Some("[ ] "),
        None => None,
    };

    let body = render_item_body(ctx, id);
    let marker_with_task: String = match task_prefix {
        Some(t) => format!("{marker}{t}"),
        None => marker.to_owned(),
    };
    let indent_width = marker_with_task.chars().count();
    // Continuation lines indent to `indent_width`; wrap the body so
    // the chosen breaks fit inside that smaller budget.
    let shrink_n = u32::try_from(indent_width).unwrap_or(u32::MAX);
    let rendered = render_to_string_with(body, ctx.opts.wrap().shrink(shrink_n));
    let trimmed = rendered.trim_end_matches('\n');
    let indent: String = std::iter::repeat_n(' ', indent_width).collect();
    let mut out = String::with_capacity(trimmed.len().saturating_add(indent_width));
    for (i, line) in trimmed.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if i == 0 {
            out.push_str(&marker_with_task);
            out.push_str(line);
        } else if line.is_empty() {
            // keep blank lines blank; no trailing spaces
        } else {
            out.push_str(&indent);
            out.push_str(line);
        }
    }
    // Emit the whole prefixed buffer as one Doc::Text inside an
    // `unbreakable` wrap; the embedded newlines pass straight
    // through the renderer. Before: per-line `to_owned()` plus a
    // HardLine node per continuation line. See `render_blockquote`.
    unbreakable(text(out))
}

/// Render an `Item`'s children. Items may carry direct inline
/// children (tight list) *or* block children (loose list, or items
/// with nested lists / code blocks). We group runs of inline kids
/// into a virtual paragraph each, recurse into block kids
/// normally, and let `render_block_sequence`'s separator discipline
/// space them apart.
///
/// When the parent list is loose (`tight = false` on the parent
/// `List` node), the item's own paragraph-level children must be
/// separated by a blank line. Otherwise pulldown re-parses two
/// paragraphs as one — the loose list becomes tight on round-trip
/// and the HTML changes from `<li><p>x</p><p>y</p></li>` to
/// `<li>x\ny</li>` (CM §5.3). Lists where every item has only one
/// block (or only inline) stay tight regardless.
fn render_item_body<'a>(ctx: &Ctx<'a>, id: NodeId) -> Doc<'a> {
    let parent_loose = ctx
        .tree
        .parent(id)
        .and_then(|p| ctx.tree.node(p))
        .is_some_and(|n| matches!(n.kind, NodeKind::List { tight: false, .. }));
    let children: Vec<NodeId> = ctx.tree.children(id).collect();
    let mut parts: Vec<Doc<'a>> = Vec::new();
    let mut inline_run: Vec<NodeId> = Vec::new();
    let mut emitted = 0usize;
    let flush_inline = |run: &mut Vec<NodeId>, parts: &mut Vec<Doc<'a>>, emitted: &mut usize| {
        if run.is_empty() {
            return;
        }
        // Previous block (if any) ended with its own `hard_line`, so
        // a one-newline separator is already in place. For a loose
        // parent list we need a *blank* line between item-internal
        // blocks — push one more `hard_line` to get two newlines.
        // For a tight parent the existing newline is exactly right
        // (CM §5.3: blocks adjacent with no blank line stay tight).
        if *emitted > 0 && parent_loose {
            parts.push(hard_line());
        }
        let inline = inline_for_children(ctx, run);
        let body = escape_paragraph_line_starts(ctx, inline);
        parts.push(concat([body, hard_line()]));
        *emitted = emitted.saturating_add(1);
        run.clear();
    };
    for cid in children {
        let kind = ctx.tree.node(cid).map(|n| &n.kind);
        if is_block_kind(kind) {
            flush_inline(&mut inline_run, &mut parts, &mut emitted);
            if emitted > 0 && parent_loose {
                // Loose list: insert a blank line between item-internal
                // blocks so each one parses back as its own block.
                parts.push(hard_line());
            }
            parts.push(render_block(ctx, cid));
            emitted = emitted.saturating_add(1);
        } else {
            inline_run.push(cid);
        }
    }
    flush_inline(&mut inline_run, &mut parts, &mut emitted);
    concat(parts)
}

/// Build a `Doc` from the inline content of a set of sibling nodes
/// (used when an Item has direct inline children — there is no
/// Paragraph wrapper to call [`render_inline`] on directly).
fn inline_for_children<'a>(ctx: &Ctx<'a>, ids: &[NodeId]) -> Doc<'a> {
    render_inline_nodes(ctx, ids)
}

fn is_block_kind(kind: Option<&NodeKind<'_>>) -> bool {
    matches!(
        kind,
        Some(
            NodeKind::Paragraph
                | NodeKind::Heading { .. }
                | NodeKind::BlockQuote
                | NodeKind::CodeBlock { .. }
                | NodeKind::HtmlBlock { .. }
                | NodeKind::ThematicBreak
                | NodeKind::List { .. }
                | NodeKind::Table { .. }
                | NodeKind::FootnoteDefinition { .. }
                | NodeKind::LinkReferenceDefinition { .. }
        )
    )
}

// ============================================================
// Tables
// ============================================================

fn render_table<'a>(ctx: &Ctx<'a>, id: NodeId, alignments: &[TableAlign]) -> Doc<'a> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row_id in ctx.tree.children(id) {
        let kind = ctx.tree.node(row_id).map(|n| &n.kind);
        if !matches!(kind, Some(NodeKind::TableHead | NodeKind::TableRow)) {
            continue;
        }
        let mut cells: Vec<String> = Vec::new();
        for cell_id in ctx.tree.children(row_id) {
            let cell_doc = render_inline(ctx, cell_id);
            let raw = render_to_string(&cell_doc);
            cells.push(normalize_table_cell(&raw));
        }
        rows.push(cells);
    }

    let n_cols = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(alignments.len());
    if n_cols == 0 {
        return concat(Vec::new());
    }
    for row in &mut rows {
        if row.len() < n_cols {
            row.resize(n_cols, String::new());
        }
    }

    let widths = compute_column_widths(&rows, alignments, n_cols, ctx.opts.wrap());

    let mut parts: Vec<Doc<'a>> = Vec::new();
    if let Some(head) = rows.first() {
        parts.push(text(format_table_row(head, &widths)));
        parts.push(hard_line());
        parts.push(text(format_alignment_row(alignments, &widths)));
        parts.push(hard_line());
    }
    for row in rows.iter().skip(1) {
        parts.push(text(format_table_row(row, &widths)));
        parts.push(hard_line());
    }
    concat(parts)
}

/// Collapse line breaks inside a cell to spaces. Pipe escaping is
/// handled upstream by the inline pass under
/// [`EscapeScope::in_table_cell`].
fn normalize_table_cell(s: &str) -> String {
    s.replace('\n', " ")
}

/// Per-column display width for emission. Width is the max of the
/// content widths (measured via `unicode-width`) and the minimum
/// alignment-marker width (`---` / `:---` / `---:` / `:---:`). The
/// total row width with `| ` chrome must stay within
/// [`Wrap::columns`]; if it would exceed, every cell falls back to
/// content width (one-space padding), matching the pre-padding
/// behaviour.
fn compute_column_widths(
    rows: &[Vec<String>],
    alignments: &[TableAlign],
    n_cols: usize,
    wrap: Wrap,
) -> Vec<usize> {
    use unicode_width::UnicodeWidthStr;

    let mut widths = vec![0usize; n_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let w = UnicodeWidthStr::width(cell.as_str());
            if let Some(slot) = widths.get_mut(i)
                && w > *slot
            {
                *slot = w;
            }
        }
    }
    for (i, slot) in widths.iter_mut().enumerate() {
        let a = alignments.get(i).copied().unwrap_or(TableAlign::None);
        let min = alignment_min_width(a);
        if min > *slot {
            *slot = min;
        }
    }

    // Row chrome: `|` opener + `| ` per column. Precisely
    // `|` + sum_i(` ` + width_i + ` |`).
    let row_width: usize = widths
        .iter()
        .map(|w| w.saturating_add(3))
        .sum::<usize>()
        .saturating_add(1);
    let target = wrap.columns() as usize;
    if row_width > target {
        // Fall back to content-width-only (no padding to column max).
        // This mirrors the pre-padding layout and prevents long
        // tables from breaking the configured wrap target.
        let mut acc = vec![0usize; n_cols];
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                let w = UnicodeWidthStr::width(cell.as_str());
                if let Some(slot) = acc.get_mut(i)
                    && w > *slot
                {
                    *slot = w;
                }
            }
        }
        return acc;
    }
    widths
}

const fn alignment_min_width(a: TableAlign) -> usize {
    match a {
        TableAlign::None => 3,                     // `---`
        TableAlign::Left | TableAlign::Right => 4, // `:---` or `---:`
        TableAlign::Center => 5,                   // `:---:`
    }
}

fn format_table_row(cells: &[String], widths: &[usize]) -> String {
    use unicode_width::UnicodeWidthStr;

    let mut out = String::from("|");
    for (i, c) in cells.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(0);
        let pad = w.saturating_sub(UnicodeWidthStr::width(c.as_str()));
        out.push(' ');
        out.push_str(c);
        for _ in 0..pad {
            out.push(' ');
        }
        out.push_str(" |");
    }
    out
}

fn format_alignment_row(alignments: &[TableAlign], widths: &[usize]) -> String {
    let mut out = String::from("|");
    for (i, &w) in widths.iter().enumerate() {
        let a = alignments.get(i).copied().unwrap_or(TableAlign::None);
        out.push(' ');
        match a {
            TableAlign::None => {
                for _ in 0..w {
                    out.push('-');
                }
            }
            TableAlign::Left => {
                out.push(':');
                for _ in 0..w.saturating_sub(1) {
                    out.push('-');
                }
            }
            TableAlign::Right => {
                for _ in 0..w.saturating_sub(1) {
                    out.push('-');
                }
                out.push(':');
            }
            TableAlign::Center => {
                out.push(':');
                for _ in 0..w.saturating_sub(2) {
                    out.push('-');
                }
                out.push(':');
            }
        }
        out.push(' ');
        out.push('|');
    }
    out
}

// ============================================================
// Footnote def & link ref def
// ============================================================

fn render_footnote_def<'a>(ctx: &Ctx<'a>, id: NodeId, label: &str) -> Doc<'a> {
    let inner = render_block_sequence(ctx, id);
    // Continuation lines indent by 4 spaces; the first line is
    // prefixed with "[^label]: " — wrap to the smaller of the two
    // so neither row exceeds the target.
    let first_prefix = label.chars().count().saturating_add(5);
    let shrink_n = u32::try_from(first_prefix.max(4)).unwrap_or(u32::MAX);
    let rendered = render_to_string_with(inner, ctx.opts.wrap().shrink(shrink_n));
    let trimmed = rendered.trim_end_matches('\n');
    let indent = "    ";
    let mut out = String::new();
    // Track open HTML-comment state across lines. Pulldown's
    // InlineHtml event for a multi-line `<!-- … -->` carries the
    // source slice verbatim, so continuation lines arrive here with
    // the footnote's 4-space continuation prefix already baked in.
    // Without compensation, our own 4-space indent stacks on top and
    // pulldown re-parses the formatted output with an 8-space
    // continuation, diverging from the source HTML. A fenced code
    // block whose body happens to start with 4 spaces (ASCII art)
    // must *not* be touched, so the compensation is gated on being
    // inside an open `<!-- … -->` span.
    let mut in_comment = false;
    for (i, line) in trimmed.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if i == 0 {
            use std::fmt::Write as _;
            let _ = write!(out, "[^{label}]: {line}");
        } else if line.is_empty() {
            // blank line stays blank
        } else if in_comment {
            let stripped = line.strip_prefix(indent).unwrap_or(line);
            out.push_str(indent);
            out.push_str(stripped);
        } else {
            out.push_str(indent);
            out.push_str(line);
        }
        in_comment = update_comment_state(in_comment, line);
    }
    // Single Doc::Text containing newlines; see `render_blockquote`.
    concat([unbreakable(text(out)), hard_line()])
}

/// Track whether the running line scan is inside an unclosed
/// `<!-- … -->` span. Only the last unmatched marker on the line
/// determines the next state — nested or overlapping comments
/// aren't representable in HTML, so this is sufficient.
fn update_comment_state(start: bool, line: &str) -> bool {
    let mut state = start;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let four = bytes.get(i..i.saturating_add(4));
        let three = bytes.get(i..i.saturating_add(3));
        if !state && four == Some(b"<!--") {
            state = true;
            i = i.saturating_add(4);
        } else if state && three == Some(b"-->") {
            state = false;
            i = i.saturating_add(3);
        } else {
            i = i.saturating_add(1);
        }
    }
    state
}

fn render_link_ref_def<'a>(
    label: &str,
    dest: &str,
    title: Option<&str>,
    style: LinkDefStyle,
) -> Doc<'a> {
    let dest_rendered = render_link_dest(dest, style);
    let line = match title {
        Some(t) => format!("[{label}]: {dest_rendered} \"{t}\""),
        None => format!("[{label}]: {dest_rendered}"),
    };
    concat([text(line), hard_line()])
}

/// Render a destination URL. Under [`LinkDefStyle::Angle`] the URL is
/// wrapped in `<…>` with `<`, `>`, and newlines escaped. Under
/// [`LinkDefStyle::Bare`] the URL is emitted unwrapped; `CommonMark`
/// requires `(`, `)`, and whitespace to be backslash-escaped.
pub(crate) fn render_link_dest(dest: &str, style: LinkDefStyle) -> String {
    let has_ws = dest
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'));
    if matches!(style, LinkDefStyle::Angle) || has_ws {
        let mut s = String::with_capacity(dest.len().saturating_add(2));
        s.push('<');
        for c in dest.chars() {
            match c {
                '<' => s.push_str("\\<"),
                '>' => s.push_str("\\>"),
                '\\' => s.push_str("\\\\"),
                '\n' | '\r' => {}
                _ => s.push(c),
            }
        }
        s.push('>');
        s
    } else {
        dest.to_owned()
    }
}

// ============================================================
// Helpers
// ============================================================

fn render_to_string(doc: &Doc<'_>) -> String {
    render(doc, &RenderOptions)
}

/// Render an inner block's `Doc` to a string under the given wrap
/// policy. Used by container blocks (blockquote, list item, footnote
/// definition) so soft-break decisions baked into the inner content
/// survive the per-line string surgery that adds prefixes.
fn render_to_string_with(doc: Doc<'_>, wrap: Wrap) -> String {
    let wrapped = wrap_doc(doc, wrap);
    render_to_string(&wrapped)
}
