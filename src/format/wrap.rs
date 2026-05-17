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
//! `Doc::Text` is one atomic box of `UnicodeWidthStr::width` — the
//! wrap pass never inspects text contents. Break candidates are
//! explicit: `Doc::Line` (source soft break) and `Doc::SoftSpace`
//! (word-boundary glue). Producers that want word-level wrappability
//! call [`crate::format::doc::prose`], which emits
//! `text(word) + SoftSpace + text(word) + …`. Producers whose syntax
//! forbids mid-line breaks (table rows, ATX heading bodies,
//! fenced-code info strings) emit plain `text(...)` and inherit
//! `Text`'s atomicity by construction — there is no "remember to
//! mark atomic" rule for callers to forget.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthStr;

use crate::config::Wrap;
use crate::format::doc::Doc;

// ============================================================
// Safety bounds
// ============================================================
//
// Wrap is `O(n²)` in box count and called on every paragraph. Real
// Markdown has paragraphs of a few dozen to a few hundred tokens;
// pathological / adversarial input can be much larger. Two defences:
//
// 1. `MAX_WRAP_TOKENS`: any paragraph with more boxes than this skips
//    the DP entirely and emits the boxes as-is (one space between
//    each). 100 000 boxes ≈ a 1 MB single paragraph of typical
//    English; well past any natural document.
// 2. `MAX_WRAP_TIME`: belt-and-suspenders. The DP itself bails out if
//    it runs longer than this and the caller falls back to the same
//    no-wrap emission. Guards against generators that produce inputs
//    we did not anticipate.

const MAX_WRAP_TOKENS: usize = 100_000;
const MAX_WRAP_TIME: Duration = Duration::from_millis(100);
/// How often (in inner-loop iterations) `solve_breaks` checks the
/// wall-clock budget. Cheap-but-not-free: `Instant::now()` is one
/// syscall on macOS; once per 1 024 inner iterations is invisible in
/// benchmarks.
const TIME_CHECK_STRIDE: usize = 1 << 10;

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
        // SoftSpace is word-boundary glue from `prose(...)`; it always
        // renders as a literal space in non-wrap modes (the source had
        // a space, not a newline). Only `Wrap::At(n)` treats it as a
        // break candidate.
        Doc::SoftSpace => Doc::Text(Cow::Borrowed(" ")),
        Doc::Text(_) | Doc::HardLine => doc,
        Doc::Atomic(inner) => Doc::Atomic(Box::new(rewrite_lines(*inner, r))),
        Doc::Prefix(p, inner) => Doc::Prefix(p, Box::new(rewrite_lines(*inner, r))),
        Doc::Concat(items) => {
            let v: Vec<Doc<'a>> = items.into_vec().into_iter().map(|i| rewrite_lines(i, r)).collect();
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
        leaf @ (Doc::Text(_) | Doc::Line | Doc::SoftSpace | Doc::HardLine | Doc::Atomic(_) | Doc::Prefix(_, _)) => {
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
                        u32::try_from(unicode_width::UnicodeWidthStr::width(p.content.as_ref())).unwrap_or(u32::MAX);
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
                Doc::Text(_) | Doc::Line | Doc::SoftSpace | Doc::Atomic(_) => {}
            }
        }
    }
    flush_run(&mut run, target, &mut out);
    out
}

/// A "wrap token" is one that participates in a run: text, soft
/// break, word-boundary glue, or atomic box.
fn is_wrap_token(d: &Doc<'_>) -> bool {
    matches!(d, Doc::Text(_) | Doc::Line | Doc::SoftSpace | Doc::Atomic(_))
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
    if boxes.len() > MAX_WRAP_TOKENS {
        tracing::warn!(
            tokens = boxes.len(),
            cap = MAX_WRAP_TOKENS,
            "wrap: paragraph exceeds token cap; emitting verbatim without re-wrap"
        );
        return rebuild_unwrapped(boxes);
    }
    match solve_breaks(&boxes, target) {
        Some(breaks) => rebuild(boxes, &breaks),
        None => {
            tracing::warn!(
                tokens = boxes.len(),
                budget_ms = MAX_WRAP_TIME.as_millis(),
                "wrap: DP time budget exceeded; emitting verbatim without re-wrap"
            );
            rebuild_unwrapped(boxes)
        }
    }
}

/// Emit boxes with one space between each, no breaks. Used as the
/// degraded path when the DP is skipped (`MAX_WRAP_TOKENS`) or bailed
/// (`MAX_WRAP_TIME`). The output is a valid `Vec<Doc>` for the caller
/// to splice in.
fn rebuild_unwrapped<'a>(boxes: Vec<Bx<'a>>) -> Vec<Doc<'a>> {
    let mut out: Vec<Doc<'a>> = Vec::with_capacity(boxes.len().saturating_mul(2));
    let mut first = true;
    for b in boxes {
        if !first {
            out.push(Doc::Text(Cow::Borrowed(" ")));
        }
        first = false;
        out.extend(b.parts);
    }
    out
}

/// Split a run into boxes with implicit glue between them.
///
/// Every `Doc::Text` is one atomic box of its display width — the
/// wrap pass never inspects text contents. `Doc::Line` and
/// `Doc::SoftSpace` are glue (break candidates). An unbreakable
/// `Doc::Atomic` is one whole box. Adjacent text/atomic nodes with
/// no intervening glue coalesce into one box — there is no break
/// candidate between them anyway, and coalescing keeps the DP cost
/// linear in the number of break candidates rather than in raw
/// `Text` count.
fn tokenize_run<'a>(run: Vec<Doc<'a>>) -> Vec<Bx<'a>> {
    let mut boxes: Vec<Bx<'a>> = Vec::new();
    let mut pending_glue = false;
    for item in run {
        match item {
            Doc::Line | Doc::SoftSpace => pending_glue = true,
            text @ Doc::Text(_) => {
                let width = doc_flat_width(&text);
                append_box(
                    &mut boxes,
                    Bx {
                        parts: vec![text],
                        width,
                    },
                    pending_glue,
                );
                pending_glue = false;
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

/// Compute the flat display width of a `Doc` subtree, treating soft
/// breaks and soft-spaces as single spaces.
fn doc_flat_width(d: &Doc<'_>) -> u32 {
    fn walk(d: &Doc<'_>, acc: &mut u32) {
        match d {
            Doc::Text(s) => {
                let w = u32::try_from(UnicodeWidthStr::width(s.as_ref())).unwrap_or(u32::MAX);
                *acc = acc.saturating_add(w);
            }
            Doc::Line | Doc::SoftSpace => *acc = acc.saturating_add(1),
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
fn solve_breaks(boxes: &[Bx<'_>], target: u32) -> Option<Vec<usize>> {
    let n = boxes.len();
    if n == 0 {
        return Some(Vec::new());
    }
    // cost[j] = min total badness for boxes[0..j]; prev[j] = best i.
    let mut cost = vec![u64::MAX; n.saturating_add(1)];
    let mut prev = vec![0usize; n.saturating_add(1)];
    if let Some(slot) = cost.get_mut(0) {
        *slot = 0;
    }
    let start = Instant::now();
    let mut tick: usize = 0;
    for j in 1..=n {
        for i in 0..j {
            tick = tick.saturating_add(1);
            if tick.is_multiple_of(TIME_CHECK_STRIDE) && start.elapsed() >= MAX_WRAP_TIME {
                return None;
            }
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
    Some(breaks)
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
        Doc, LinePrefix, RenderOptions, concat, hard_line, line, prefix_lines, render, text, unbreakable,
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
    fn at_target_does_not_split_a_single_text() {
        // `Doc::Text` is atomic by the Wadler/Lindig discipline: the
        // wrap pass never inspects its contents. A long text with
        // internal spaces stays on one line (forced overflow) when
        // no `Doc::Line` / `Doc::SoftSpace` declares break
        // opportunities. Producers expressing word-level wrappability
        // use `crate::format::doc::prose`; see the next test.
        let d = text("aaa bbb ccc ddd eee");
        assert_eq!(render_wrapped(d, Wrap::At(8)), "aaa bbb ccc ddd eee");
    }

    #[test]
    fn at_target_breaks_prose_at_explicit_soft_spaces() {
        use crate::format::doc::prose;
        let d = prose("aaa bbb ccc ddd eee");
        assert_eq!(render_wrapped(d, Wrap::At(8)), "aaa bbb\nccc ddd\neee");
    }

    #[test]
    fn keep_renders_soft_space_as_literal_space() {
        use crate::format::doc::prose;
        // `Wrap::Keep` must NOT promote `SoftSpace` to `HardLine`
        // (only `Line`, the source soft break, does that). A prose
        // tokenisation that all fits on one line in source must stay
        // on one line in output.
        let d = prose("aaa bbb ccc");
        assert_eq!(render_wrapped(d, Wrap::Keep), "aaa bbb ccc");
    }

    #[test]
    fn unbreakable_group_stays_whole_even_when_oversize() {
        let d = concat([text("hi"), line(), unbreakable(text("xxxxxxxxxx"))]);
        assert_eq!(render_wrapped(d, Wrap::At(5)), "hi\nxxxxxxxxxx");
    }

    #[test]
    fn hard_line_terminates_run() {
        let d = concat([text("foo"), line(), text("bar"), hard_line(), text("baz qux")]);
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
        let breaks = super::solve_breaks(&boxes, 7).unwrap_or_default();
        assert_eq!(breaks, vec![2, 4]);
    }
}
