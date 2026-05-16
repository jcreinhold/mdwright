//! Paragraph wrap pass: `Doc → Doc`.
//!
//! Runs after the block/inline serializers and before the renderer.
//! Decides where to place line breaks inside a paragraph or heading,
//! never splitting an `unbreakable` group (inline code, link, autolink,
//! raw HTML). The renderer downstream sees only `Doc::Text`,
//! `Doc::HardLine`, and atomic groups; soft `Doc::Line` markers are
//! gone.
//!
//! Three modes:
//!
//! - [`Wrap::Keep`] — preserve every source `Doc::Line` as a
//!   `Doc::HardLine`. The renderer with `width = u32::MAX` would
//!   otherwise collapse them to spaces.
//! - [`Wrap::No`] — replace every `Doc::Line` with a single space.
//!   Paragraphs collapse to one line.
//! - [`Wrap::At(n)`] — Knuth-Plass-lite DP per "run" (a contiguous
//!   sub-sequence of wrappable tokens delimited by `HardLine` or
//!   any opaque non-wrappable node). Boxes are words and atomic
//!   groups; the cost of a line is `(target - width)²` when the line
//!   fits, infinity (with a finite overflow penalty for forced lines)
//!   when it does not.
//!
//! ## Box / glue model
//!
//! A run is split into a sequence of boxes (atomic content) and glue
//! (whitespace where a break may be placed). Box widths are summed
//! by [`unicode_width::UnicodeWidthStr`] so math glyphs and CJK do
//! not under-count. Glue is a single column when kept, zero columns
//! when chosen as a break.
//!
//! ## Tokenisation
//!
//! `Doc::Text` is split on ASCII whitespace into one box per word.
//! This makes wrap insensitive to whether the source used a soft
//! break or a space between two words — both produce identical box
//! streams.

use std::borrow::Cow;

use unicode_width::UnicodeWidthStr;

use crate::config::Wrap;
use crate::format::doc::Doc;

// ============================================================
// Public entry
// ============================================================

/// Apply the wrap policy to `doc`. Idempotent: a `Doc` with no
/// `Doc::Line` is returned unchanged.
pub(crate) fn wrap_doc(doc: Doc<'_>, wrap: Wrap) -> Doc<'_> {
    match wrap {
        Wrap::Keep => rewrite_lines(doc, Replace::HardLine),
        Wrap::No => rewrite_lines(doc, Replace::Space),
        Wrap::At(target) => wrap_at(doc, target.max(1)),
    }
}

// ============================================================
// Keep / No: simple recursive substitution
// ============================================================

#[derive(Copy, Clone)]
enum Replace {
    HardLine,
    Space,
}

fn rewrite_lines<'a>(doc: Doc<'a>, r: Replace) -> Doc<'a> {
    match doc {
        Doc::Line => match r {
            Replace::HardLine => Doc::HardLine,
            Replace::Space => Doc::Text(Cow::Borrowed(" ")),
        },
        Doc::Text(_) | Doc::HardLine => doc,
        Doc::Atomic(inner) => Doc::Atomic(Box::new(rewrite_lines(*inner, r))),
        Doc::Prefix(p, inner) => Doc::Prefix(p, Box::new(rewrite_lines(*inner, r))),
        Doc::Concat(items) => {
            let v: Vec<Doc<'a>> = items
                .into_vec()
                .into_iter()
                .map(|i| rewrite_lines(i, r))
                .collect();
            Doc::Concat(v.into_boxed_slice())
        }
    }
}

// ============================================================
// At(n): per-run Knuth-Plass-lite
// ============================================================

fn wrap_at<'a>(doc: Doc<'a>, target: u32) -> Doc<'a> {
    let mut stream: Vec<Doc<'a>> = Vec::new();
    linearize(doc, &mut stream);
    let out = process_stream(stream, target);
    if out.len() == 1 {
        let mut v = out;
        // Single element — peel the Concat wrapper.
        v.pop().unwrap_or_else(|| Doc::Concat(Box::new([])))
    } else {
        Doc::Concat(out.into_boxed_slice())
    }
}

/// Flatten top-level `Concat` trees into a linear list, preserving
/// every other node opaquely. The wrap algorithm operates over this
/// linear view.
fn linearize<'a>(doc: Doc<'a>, out: &mut Vec<Doc<'a>>) {
    match doc {
        Doc::Concat(items) => {
            for item in items.into_vec() {
                linearize(item, out);
            }
        }
        leaf @ (Doc::Text(_) | Doc::Line | Doc::HardLine | Doc::Atomic(_) | Doc::Prefix(_, _)) => {
            out.push(leaf);
        }
    }
}

fn process_stream<'a>(stream: Vec<Doc<'a>>, target: u32) -> Vec<Doc<'a>> {
    let mut out: Vec<Doc<'a>> = Vec::with_capacity(stream.len());
    let mut run: Vec<Doc<'a>> = Vec::new();
    for item in stream {
        if is_wrap_token(&item) {
            run.push(item);
        } else {
            flush_run(&mut run, target, &mut out);
            match item {
                // Recurse into the prefix subtree with a reduced
                // budget so continuation-line wrapping accounts for
                // the prefix's column cost.
                Doc::Prefix(p, inner) => {
                    let shrink =
                        u32::try_from(unicode_width::UnicodeWidthStr::width(p.content.as_ref()))
                            .unwrap_or(u32::MAX);
                    let new_target = target.saturating_sub(shrink).max(1);
                    let wrapped = wrap_at(*inner, new_target);
                    out.push(Doc::Prefix(p, Box::new(wrapped)));
                }
                // `HardLine` and (defensively) `Concat` end up here:
                // `Text`, `Line`, and `Atomic` are wrap tokens and
                // were siphoned into `run` by `is_wrap_token`.
                other @ (Doc::HardLine | Doc::Concat(_)) => out.push(other),
                // Wrap-tokens cannot reach the `else` branch — they
                // were filtered into the run above. Drop defensively.
                Doc::Text(_) | Doc::Line | Doc::Atomic(_) => {}
            }
        }
    }
    flush_run(&mut run, target, &mut out);
    out
}

/// A "wrap token" is one that participates in a run: text, soft
/// break, or atomic box.
fn is_wrap_token(d: &Doc<'_>) -> bool {
    matches!(d, Doc::Text(_) | Doc::Line | Doc::Atomic(_))
}

fn flush_run<'a>(run: &mut Vec<Doc<'a>>, target: u32, out: &mut Vec<Doc<'a>>) {
    if run.is_empty() {
        return;
    }
    let taken = std::mem::take(run);
    let wrapped = wrap_run(taken, target);
    out.extend(wrapped);
}

// ============================================================
// The DP itself
// ============================================================

/// A "box" in line-breaking terms: an atomic unit of content with a
/// fixed display width that cannot be split across lines.
struct Bx<'a> {
    /// Doc nodes that compose this box, in emission order. May be a
    /// single `Doc::Text` (one word), several texts coalesced from
    /// adjacent siblings without intervening glue, or an
    /// unbreakable `Doc::Group`.
    parts: Vec<Doc<'a>>,
    /// Display width in columns.
    width: u32,
}

fn wrap_run(run: Vec<Doc<'_>>, target: u32) -> Vec<Doc<'_>> {
    let boxes = tokenize_run(run);
    if boxes.is_empty() {
        return Vec::new();
    }
    if boxes.len() == 1 {
        return boxes.into_iter().next().map_or_else(Vec::new, |b| b.parts);
    }
    let breaks = solve_breaks(&boxes, target);
    rebuild(boxes, &breaks)
}

/// Split a run into boxes with implicit glue between them.
///
/// A `Doc::Text` is split on ASCII whitespace; every maximal
/// non-whitespace span becomes a word-box, every whitespace span
/// marks pending glue between boxes. A `Doc::Line` is glue. An
/// unbreakable `Doc::Group` is one whole box. Adjacent content with
/// no intervening glue is coalesced into one box — there is no
/// breakpoint between them anyway.
fn tokenize_run<'a>(run: Vec<Doc<'a>>) -> Vec<Bx<'a>> {
    let mut boxes: Vec<Bx<'a>> = Vec::new();
    let mut pending_glue = false;
    for item in run {
        match item {
            Doc::Line => pending_glue = true,
            Doc::Text(s) => {
                // Borrowed Cow: each word becomes a Cow::Borrowed
                // slice into the original source — zero allocs per
                // word. Owned Cow (rare; pulldown delivers it for
                // entity-decoded text): fall back to per-word
                // `to_owned`. See `format/corpus/wrap-100` bench.
                match s {
                    Cow::Borrowed(src) => tokenize_borrowed(src, &mut boxes, &mut pending_glue),
                    Cow::Owned(owned) => tokenize_owned(&owned, &mut boxes, &mut pending_glue),
                }
            }
            other @ Doc::Atomic(_) => {
                let width = doc_flat_width(&other);
                append_box(
                    &mut boxes,
                    Bx {
                        parts: vec![other],
                        width,
                    },
                    pending_glue,
                );
                pending_glue = false;
            }
            // Defensive: process_stream routes `HardLine`, `Prefix`,
            // and `Concat` outside runs, so none should reach here.
            // Treat any leak as an atomic box of its flat width.
            other @ (Doc::HardLine | Doc::Concat(_) | Doc::Prefix(_, _)) => {
                let width = doc_flat_width(&other);
                append_box(
                    &mut boxes,
                    Bx {
                        parts: vec![other],
                        width,
                    },
                    pending_glue,
                );
                pending_glue = false;
            }
        }
    }
    boxes
}

/// Tokenize a borrowed source slice into boxes — each word is a
/// `Cow::Borrowed` substring (no allocation).
fn tokenize_borrowed<'a>(src: &'a str, boxes: &mut Vec<Bx<'a>>, pending_glue: &mut bool) {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let ws_start = i;
        while i < bytes.len() && is_ws(bytes.get(i).copied().unwrap_or(0)) {
            i = i.saturating_add(1);
        }
        if i > ws_start {
            *pending_glue = true;
        }
        if i >= bytes.len() {
            break;
        }
        let w_start = i;
        while i < bytes.len() && !is_ws(bytes.get(i).copied().unwrap_or(0)) {
            i = i.saturating_add(1);
        }
        let word: &'a str = src.get(w_start..i).unwrap_or("");
        let width = u32::try_from(UnicodeWidthStr::width(word)).unwrap_or(u32::MAX);
        let bx = Bx {
            parts: vec![Doc::Text(Cow::Borrowed(word))],
            width,
        };
        append_box(boxes, bx, *pending_glue);
        *pending_glue = false;
    }
}

/// Tokenize an owned String: each word must be cloned out since the
/// String dies with the call. Kept for parity with the borrowed path.
fn tokenize_owned(src: &str, boxes: &mut Vec<Bx<'_>>, pending_glue: &mut bool) {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let ws_start = i;
        while i < bytes.len() && is_ws(bytes.get(i).copied().unwrap_or(0)) {
            i = i.saturating_add(1);
        }
        if i > ws_start {
            *pending_glue = true;
        }
        if i >= bytes.len() {
            break;
        }
        let w_start = i;
        while i < bytes.len() && !is_ws(bytes.get(i).copied().unwrap_or(0)) {
            i = i.saturating_add(1);
        }
        let word = src.get(w_start..i).unwrap_or("");
        let width = u32::try_from(UnicodeWidthStr::width(word)).unwrap_or(u32::MAX);
        let bx = Bx {
            parts: vec![Doc::Text(Cow::Owned(word.to_owned()))],
            width,
        };
        append_box(boxes, bx, *pending_glue);
        *pending_glue = false;
    }
}

/// Append `b` to `boxes`. If `break_before` (a glue position
/// preceded this box) and `boxes` is non-empty, `b` starts a new
/// box; otherwise it is coalesced into the previous box.
fn append_box<'a>(boxes: &mut Vec<Bx<'a>>, b: Bx<'a>, break_before: bool) {
    if b.parts.is_empty() && b.width == 0 {
        return;
    }
    if break_before || boxes.is_empty() {
        boxes.push(b);
    } else if let Some(last) = boxes.last_mut() {
        last.parts.extend(b.parts);
        last.width = last.width.saturating_add(b.width);
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// Compute the flat display width of a `Doc` subtree, treating soft
/// breaks as single spaces.
fn doc_flat_width(d: &Doc<'_>) -> u32 {
    fn walk(d: &Doc<'_>, acc: &mut u32) {
        match d {
            Doc::Text(s) => {
                let w = u32::try_from(UnicodeWidthStr::width(s.as_ref())).unwrap_or(u32::MAX);
                *acc = acc.saturating_add(w);
            }
            Doc::Line => *acc = acc.saturating_add(1),
            Doc::HardLine => {}
            Doc::Atomic(inner) | Doc::Prefix(_, inner) => walk(inner, acc),
            Doc::Concat(items) => {
                for item in items {
                    walk(item, acc);
                }
            }
        }
    }
    let mut acc = 0u32;
    walk(d, &mut acc);
    acc
}

/// Cost of a single oversized line, added on top of `(width - target)²`
/// when no breakpoint can keep the line within `target` (a single box
/// wider than the target is forced).
const OVERFLOW_PENALTY: u64 = 1_000_000;

/// Knuth-Plass-lite: choose a sequence of break positions minimising
/// sum of squared slack.
///
/// Returns the indices `[i₁, i₂, …, n]` such that lines are
/// `boxes[0..i₁]`, `boxes[i₁..i₂]`, …, `boxes[iₖ..n]`. The terminal
/// index is always `n = boxes.len()`.
fn solve_breaks(boxes: &[Bx<'_>], target: u32) -> Vec<usize> {
    let n = boxes.len();
    if n == 0 {
        return Vec::new();
    }
    // cost[j] = min total badness for boxes[0..j]; prev[j] = best i.
    let mut cost = vec![u64::MAX; n.saturating_add(1)];
    let mut prev = vec![0usize; n.saturating_add(1)];
    if let Some(slot) = cost.get_mut(0) {
        *slot = 0;
    }
    for j in 1..=n {
        for i in 0..j {
            let base = cost.get(i).copied().unwrap_or(u64::MAX);
            if base == u64::MAX {
                continue;
            }
            let line_w = line_width(boxes, i, j);
            let bad = badness(line_w, target, j == n, j.saturating_sub(i));
            let total = base.saturating_add(bad);
            if total < cost.get(j).copied().unwrap_or(u64::MAX) {
                if let Some(slot) = cost.get_mut(j) {
                    *slot = total;
                }
                if let Some(slot) = prev.get_mut(j) {
                    *slot = i;
                }
            }
        }
    }
    // Trace back.
    let mut breaks: Vec<usize> = Vec::new();
    let mut j = n;
    while j > 0 {
        breaks.push(j);
        j = prev.get(j).copied().unwrap_or(0);
    }
    breaks.reverse();
    breaks
}

fn line_width(boxes: &[Bx<'_>], i: usize, j: usize) -> u32 {
    let slice = boxes.get(i..j).unwrap_or(&[]);
    let words: u32 = slice.iter().map(|b| b.width).sum();
    // One inter-word space per glue position: (j - i - 1) when at
    // least one box is on the line.
    let glues = u32::try_from(j.saturating_sub(i).saturating_sub(1)).unwrap_or(0);
    words.saturating_add(glues)
}

fn badness(line_w: u32, target: u32, is_last_line: bool, boxes_on_line: usize) -> u64 {
    if line_w <= target {
        if is_last_line {
            // Don't penalise short last lines; otherwise the DP fights
            // to spread content evenly across the last two lines.
            return 0;
        }
        let slack = u64::from(target.saturating_sub(line_w));
        slack.saturating_mul(slack)
    } else if boxes_on_line <= 1 {
        // A single oversized box: forced, finite penalty.
        let over = u64::from(line_w.saturating_sub(target));
        OVERFLOW_PENALTY.saturating_add(over.saturating_mul(over))
    } else {
        // Multiple boxes that don't fit: prohibited; another
        // breakpoint must be chosen.
        u64::MAX
    }
}

fn rebuild<'a>(boxes: Vec<Bx<'a>>, breaks: &[usize]) -> Vec<Doc<'a>> {
    let mut out: Vec<Doc<'a>> = Vec::with_capacity(boxes.len().saturating_mul(2));
    let mut iter = boxes.into_iter();
    let mut consumed = 0usize;
    for (k, &end) in breaks.iter().enumerate() {
        let is_last_line = k.saturating_add(1) == breaks.len();
        let mut first = true;
        while consumed < end {
            if !first {
                out.push(Doc::Text(Cow::Borrowed(" ")));
            }
            first = false;
            if let Some(b) = iter.next() {
                out.extend(b.parts);
            }
            consumed = consumed.saturating_add(1);
        }
        if !is_last_line {
            out.push(Doc::HardLine);
        }
    }
    out
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::{Wrap, wrap_doc};
    use crate::format::doc::{
        Doc, LinePrefix, RenderOptions, concat, hard_line, line, prefix_lines, render, text,
        unbreakable,
    };

    fn render_wrapped(doc: Doc<'_>, wrap: Wrap) -> String {
        let wrapped = wrap_doc(doc, wrap);
        render(&wrapped, &RenderOptions)
    }

    #[test]
    fn keep_rewrites_line_to_hardline() {
        let d = concat([text("a"), line(), text("b")]);
        assert_eq!(render_wrapped(d, Wrap::Keep), "a\nb");
    }

    #[test]
    fn no_rewrites_line_to_space() {
        let d = concat([text("a"), line(), text("b")]);
        assert_eq!(render_wrapped(d, Wrap::No), "a b");
    }

    #[test]
    fn at_target_does_not_break_when_short() {
        let d = concat([text("hi"), line(), text("there")]);
        assert_eq!(render_wrapped(d, Wrap::At(80)), "hi there");
    }

    #[test]
    fn at_target_breaks_to_fit() {
        // Two words of total width 10 ("foo" + " " + "barbaz" = 10).
        // Target 5 forces a break between them.
        let d = concat([text("foo"), line(), text("barbaz")]);
        assert_eq!(render_wrapped(d, Wrap::At(5)), "foo\nbarbaz");
    }

    #[test]
    fn at_target_breaks_long_single_text_on_whitespace() {
        // Source has no soft break, only spaces in one text node.
        let d = text("aaa bbb ccc ddd eee");
        // Target 8 lets at most two 3-letter words per line.
        let out = render_wrapped(d, Wrap::At(8));
        // Knuth-Plass minimises squared slack; with target 8 every
        // line of "aaa bbb" (width 7) has slack 1 (cost 1).
        assert_eq!(out, "aaa bbb\nccc ddd\neee");
    }

    #[test]
    fn unbreakable_group_stays_whole_even_when_oversize() {
        let d = concat([text("hi"), line(), unbreakable(text("xxxxxxxxxx"))]);
        assert_eq!(render_wrapped(d, Wrap::At(5)), "hi\nxxxxxxxxxx");
    }

    #[test]
    fn hard_line_terminates_run() {
        let d = concat([
            text("foo"),
            line(),
            text("bar"),
            hard_line(),
            text("baz qux"),
        ]);
        let out = render_wrapped(d, Wrap::At(80));
        assert_eq!(out, "foo bar\nbaz qux");
    }

    #[test]
    fn unicode_width_counts_wide_glyphs() {
        // CJK characters are width-2 each. "你好" is 4 columns wide
        // (2 chars × 2), so at target 4 it fits alone on a line.
        let d = concat([text("你好"), line(), text("你好")]);
        assert_eq!(render_wrapped(d, Wrap::At(4)), "你好\n你好");
    }

    #[test]
    fn empty_input_returns_empty() {
        let d: Doc<'_> = concat([]);
        assert_eq!(render_wrapped(d, Wrap::At(80)), "");
    }

    #[test]
    fn wrap_recurses_into_prefix_with_shrunken_target() {
        // Inner content fits at outer target 10 but not at the
        // reduced target (10 - 2 = 8) the "> " prefix imposes.
        // Words "aaaa bbbb": flat width 9 (fits at 10, not at 8).
        let inner = concat([text("aaaa"), line(), text("bbbb")]);
        let prefixed = prefix_lines(
            LinePrefix {
                content: "> ".into(),
                blank: ">".into(),
            },
            inner,
        );
        let out = render_wrapped(prefixed, Wrap::At(10));
        // Reduced target forces the break; continuation line carries
        // the "> " drain.
        assert_eq!(out, "aaaa\n> bbbb");
    }

    #[test]
    fn prefix_keep_mode_preserves_inner_breaks() {
        let inner = concat([text("a"), line(), text("b")]);
        let prefixed = prefix_lines(
            LinePrefix {
                content: "> ".into(),
                blank: ">".into(),
            },
            inner,
        );
        // Keep promotes Line to HardLine inside Prefix too.
        assert_eq!(render_wrapped(prefixed, Wrap::Keep), "a\n> b");
    }

    #[test]
    fn solver_minimises_squared_slack() {
        // Boxes of widths [3, 3, 3, 3] with target 7: best layout
        // is "3 3 / 3 3" (slack 1+0); the alternative "3 3 3 / 3"
        // would overflow (width 11 > 7). The DP must pick the
        // two-per-line layout.
        let boxes = vec![
            super::Bx {
                parts: vec![text("aaa")],
                width: 3,
            },
            super::Bx {
                parts: vec![text("bbb")],
                width: 3,
            },
            super::Bx {
                parts: vec![text("ccc")],
                width: 3,
            },
            super::Bx {
                parts: vec![text("ddd")],
                width: 3,
            },
        ];
        let breaks = super::solve_breaks(&boxes, 7);
        assert_eq!(breaks, vec![2, 4]);
    }
}
