//! The `Doc` combinator IR and its renderer.
//!
//! A `Doc` is a tree of layout instructions. The wrap pass
//! ([`crate::format::wrap`]) makes all break decisions before the
//! renderer runs; the renderer then walks the tree once and emits a
//! string.
//!
//! ## Semantics
//!
//! - [`Doc::Text(s)`](Doc::Text) — literal text; must not contain
//!   `\n`. **Atomic**: the wrap pass treats every `Text` as one
//!   indivisible box of `UnicodeWidthStr::width(s)` columns and never
//!   inspects its contents. Producers that want word-level
//!   wrappability call [`prose`] to emit a stream of
//!   `text + soft_space + text + soft_space + …` boxes; emitters
//!   whose syntax must stay on one line (table rows, ATX heading
//!   bodies, fenced-code info strings) use plain `text(...)` and
//!   inherit its atomicity by default.
//! - [`Doc::Line`] — source soft break. In `Wrap::Keep` it promotes
//!   to a `HardLine` (preserving the source's newline); in
//!   `Wrap::No` it collapses to a literal `" "`; in `Wrap::At(n)`
//!   it is a break candidate. The wrap pass converts soft breaks
//!   chosen as breaks into `HardLine` before render time.
//! - [`Doc::SoftSpace`] — word-boundary glue introduced by
//!   [`prose`]. Always renders as a single space in `Wrap::Keep` and
//!   `Wrap::No` (the source had a literal space, not a newline); in
//!   `Wrap::At(n)` it is a break candidate. The distinction from
//!   `Line` is what lets `Keep` mode preserve source line shape
//!   without exploding every prose word onto its own line.
//! - [`Doc::HardLine`] — newline. Subsequent content on the new line
//!   may carry any enclosing [`Doc::Prefix`] drains.
//! - [`Doc::Atomic`] — atomic group; the wrap pass refuses to break
//!   at any `Line` or `SoftSpace` inside the wrapped subtree. Used
//!   for *multi-element* atomic groups (link form `[text](dest)`,
//!   fenced code block whose interior `HardLine`s are syntactic).
//!   Single `Text` strings do not need `Atomic` — they are already
//!   atomic.
//! - [`Doc::Concat`] — render children in order.
//! - [`Doc::Prefix(p, inner)`](Doc::Prefix) — every hard line inside
//!   `inner` is prepended with `p.content` (or `p.blank` for the
//!   "middle" of a blank line). The first line of `inner` is *not*
//!   prefixed — the caller is expected to pre-pend the first-line
//!   form (e.g. `concat([text("> "), prefix_lines(…, body)])`).
//!   Trailing prefixes don't drain: a `Doc::Prefix` whose `inner`
//!   ends in a `HardLine` does not emit `p.content` after that final
//!   newline, so the natural truncation matches the legacy
//!   blockquote's trim-trailing-marker behavior.

use std::borrow::Cow;

/// What to emit at line boundaries inside a [`Doc::Prefix`] subtree.
/// `content` is prepended before the next non-empty leaf; `blank`
/// is prepended before a second `HardLine` (i.e. on the "middle" of
/// a blank line). Strings are formatter-chosen, not source-derived,
/// so the lifetime is `'static`.
#[derive(Clone, Debug)]
pub(crate) struct LinePrefix {
    pub content: Cow<'static, str>,
    pub blank: Cow<'static, str>,
}

/// Tree of layout instructions; see the module docs.
#[derive(Clone, Debug)]
pub(crate) enum Doc<'a> {
    Text(Cow<'a, str>),
    Line,
    SoftSpace,
    HardLine,
    Atomic(Box<Self>),
    Concat(Box<[Self]>),
    Prefix(LinePrefix, Box<Self>),
}

/// Knobs for [`render`]. Currently empty: layout decisions live in
/// the wrap pass and the renderer is parameter-free. Kept as a
/// distinct type so future width- or indent-sensitive renderers do
/// not change the call signature.
#[derive(Clone, Debug, Default)]
pub(crate) struct RenderOptions;

// --- constructors ---------------------------------------------------

/// `Doc::Text` is CR-free by construction. Every source-passthrough
/// emit site (setext heading body, fenced/indented code body, HTML
/// block body, frontmatter, admonitions, paragraph verbatim copy)
/// flows through here; canonicalising EOL once at construction means
/// downstream width calculations and the rendered byte stream agree,
/// and the post-render line-ending normaliser becomes redundant.
///
/// NUL is canonicalised once at parse time inside
/// [`crate::source::Source::new`] (CM §2.3 NUL → U+FFFD) and never
/// reaches this function. Pulldown sees canonical bytes and the
/// formatter emits canonical bytes; the runtime HTML gate
/// (`Document::format_validated`) compares canonical-bytes-in vs
/// canonical-bytes-out so the NUL-vs-FFFD flanking divergence that
/// previously caused bug-class G cannot arise.
///
/// Cost: `.contains('\r')` early-out; zero allocation for the common
/// case (synthesised prefixes like `# `, escape `\`, and any source
/// slice without `\r`).
pub(crate) fn text<'a>(s: impl Into<Cow<'a, str>>) -> Doc<'a> {
    let s = s.into();
    if s.contains('\r') {
        let normalised = s.replace("\r\n", "\n").replace('\r', "\n");
        Doc::Text(Cow::Owned(normalised))
    } else {
        Doc::Text(s)
    }
}

pub(crate) fn line<'a>() -> Doc<'a> {
    Doc::Line
}

pub(crate) fn hard_line<'a>() -> Doc<'a> {
    Doc::HardLine
}

pub(crate) fn unbreakable(inner: Doc<'_>) -> Doc<'_> {
    Doc::Atomic(Box::new(inner))
}

pub(crate) fn concat<'a>(items: impl IntoIterator<Item = Doc<'a>>) -> Doc<'a> {
    Doc::Concat(items.into_iter().collect::<Vec<_>>().into_boxed_slice())
}

/// Tokenise prose into a wrap-friendly `Doc`: every whitespace-
/// separated word becomes a [`Doc::Text`] box, with [`Doc::SoftSpace`]
/// (word-boundary glue) between them. Source whitespace runs collapse
/// to a single `SoftSpace`; leading and trailing whitespace are
/// preserved as boundary `SoftSpace`s so the result composes with
/// neighbouring constructs without losing the gap.
///
/// This is the single point where prose producers communicate "the
/// wrap pass may break between these words." Producers whose syntax
/// forbids mid-line breaks (table rows, ATX heading bodies,
/// fenced-code info strings) keep using [`text`] directly and inherit
/// `Text`'s atomicity.
pub(crate) fn prose<'a>(s: &str) -> Doc<'a> {
    // Whitespace set deliberately narrower than `char::is_ascii_whitespace`:
    // we treat only space, tab, newline, and carriage return as word
    // separators. Form-feed (`\f`) and vertical-tab (`\x0B`) bytes
    // appear in text payloads (CommonMark §2.1 leaves them as data
    // bytes; pulldown surfaces them inside `Event::Text`) and must
    // pass through as literal content — folding them to spaces would
    // break round-trip on inputs that contain control bytes.
    fn is_word_break(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\n' | b'\r')
    }
    let bytes = s.as_bytes();
    let leading = bytes.first().is_some_and(|b| is_word_break(*b));
    let trailing = bytes.len() > 1 && bytes.last().is_some_and(|b| is_word_break(*b));
    let mut parts: Vec<Doc<'a>> = Vec::new();
    if leading {
        parts.push(Doc::SoftSpace);
    }
    let mut first = true;
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes.get(i).is_some_and(|b| is_word_break(*b)) {
            i = i.saturating_add(1);
        }
        if i >= bytes.len() {
            break;
        }
        let w_start = i;
        while i < bytes.len() && bytes.get(i).is_some_and(|b| !is_word_break(*b)) {
            i = i.saturating_add(1);
        }
        if !first {
            parts.push(Doc::SoftSpace);
        }
        first = false;
        let word = s.get(w_start..i).unwrap_or("");
        parts.push(Doc::Text(Cow::Owned(word.to_owned())));
    }
    if trailing && !first {
        parts.push(Doc::SoftSpace);
    }
    if parts.is_empty() {
        Doc::Concat(Box::new([]))
    } else if parts.len() == 1 {
        parts.into_iter().next().unwrap_or_else(|| Doc::Concat(Box::new([])))
    } else {
        Doc::Concat(parts.into_boxed_slice())
    }
}

pub(crate) fn prefix_lines(p: LinePrefix, inner: Doc<'_>) -> Doc<'_> {
    Doc::Prefix(p, Box::new(inner))
}

// --- renderer -------------------------------------------------------

/// One unit of work on the renderer's walk stack. `PopPrefix` pairs
/// with a `Render(Prefix(…))` entry: the prefix is pushed when the
/// `Render` fires and popped when the matching `PopPrefix` fires
/// after the inner subtree has been fully emitted.
enum Op<'b, 'a> {
    Render(&'b Doc<'a>),
    PopPrefix,
}

/// State tracking the gap between an emitted `\n` and the next leaf
/// that decides whether the enclosing prefixes drain in their
/// `content` form (next leaf is real content) or `blank` form (next
/// op is another `HardLine`).
#[derive(Copy, Clone, PartialEq, Eq)]
enum Pending {
    None,
    AfterHardLine,
}

/// Render `doc` into a `String` using `opts`. Soft `Doc::Line`
/// markers render as a single space; hard breaks render as a bare
/// newline followed by any enclosing [`Doc::Prefix`] drains.
pub(crate) fn render(doc: &Doc<'_>, _opts: &RenderOptions) -> String {
    let mut out = String::new();
    let mut stack: Vec<Op<'_, '_>> = Vec::with_capacity(16);
    let mut prefixes: Vec<&LinePrefix> = Vec::new();
    let mut pending = Pending::None;
    stack.push(Op::Render(doc));

    while let Some(op) = stack.pop() {
        match op {
            Op::PopPrefix => {
                prefixes.pop();
            }
            Op::Render(d) => match d {
                Doc::Text(s) => {
                    emit_text(&mut out, s, &prefixes, &mut pending);
                }
                Doc::Line | Doc::SoftSpace => {
                    // Both render as a single space at render time.
                    // `SoftSpace` is always a literal space; `Line`
                    // is what the wrap pass left as flat (the breaks
                    // it chose were already converted to `HardLine`).
                    drain_content(&mut out, &prefixes, &mut pending);
                    out.push(' ');
                }
                Doc::HardLine => {
                    if pending == Pending::AfterHardLine {
                        // Back-to-back hard lines: the line in between
                        // is "blank" — emit blank-form prefixes before
                        // the second newline so the rendered blank
                        // line carries the right marker (e.g. `>` not
                        // `> `).
                        drain_blank(&mut out, &prefixes);
                    }
                    out.push('\n');
                    pending = Pending::AfterHardLine;
                }
                Doc::Atomic(inner) => {
                    drain_content(&mut out, &prefixes, &mut pending);
                    stack.push(Op::Render(inner));
                }
                Doc::Concat(items) => {
                    for item in items.iter().rev() {
                        stack.push(Op::Render(item));
                    }
                }
                Doc::Prefix(p, inner) => {
                    // Push the pop-marker first so it fires after the
                    // inner subtree completes; then push the inner;
                    // then activate the prefix immediately so it
                    // applies to drains that originate from inner.
                    stack.push(Op::PopPrefix);
                    stack.push(Op::Render(inner));
                    prefixes.push(p);
                }
            },
        }
    }

    out
}

fn drain_content(out: &mut String, prefixes: &[&LinePrefix], pending: &mut Pending) {
    if *pending != Pending::AfterHardLine {
        return;
    }
    for p in prefixes {
        out.push_str(&p.content);
    }
    *pending = Pending::None;
}

fn drain_blank(out: &mut String, prefixes: &[&LinePrefix]) {
    for p in prefixes {
        out.push_str(&p.blank);
    }
}

/// Emit a [`Doc::Text`] payload that may contain embedded `\n` bytes,
/// treating each `\n` as a logical [`Doc::HardLine`] for prefix-drain purposes.
/// Constructs like [`crate::cm::block::code::FencedCodeBlock`] pack
/// multi-line bodies into a single `Text` wrapped in `Atomic`; under
/// a [`Doc::Prefix`] subtree those internal newlines still need the
/// prefix applied.
fn emit_text(out: &mut String, s: &str, prefixes: &[&LinePrefix], pending: &mut Pending) {
    if prefixes.is_empty() || !s.contains('\n') {
        drain_content(out, prefixes, pending);
        out.push_str(s);
        return;
    }
    let mut iter = s.split('\n');
    let first = iter.next().unwrap_or("");
    if !first.is_empty() || *pending == Pending::AfterHardLine {
        drain_content(out, prefixes, pending);
        out.push_str(first);
    }
    for seg in iter {
        // We're emitting a `\n` now. If the previous "line" was blank
        // and we're still pending from before, drain blank-form.
        if *pending == Pending::AfterHardLine {
            drain_blank(out, prefixes);
        }
        out.push('\n');
        *pending = Pending::AfterHardLine;
        if !seg.is_empty() {
            drain_content(out, prefixes, pending);
            out.push_str(seg);
        }
    }
}

// --- tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{Doc, LinePrefix, RenderOptions, concat, hard_line, line, prefix_lines, render, text, unbreakable};

    fn r(doc: &Doc<'_>) -> String {
        render(doc, &RenderOptions)
    }

    fn quote_prefix() -> LinePrefix {
        LinePrefix {
            content: "> ".into(),
            blank: ">".into(),
        }
    }

    fn indent4_prefix() -> LinePrefix {
        LinePrefix {
            content: "    ".into(),
            blank: "".into(),
        }
    }

    #[test]
    fn empty_concat_renders_to_empty() {
        let d: Doc<'_> = concat([]);
        assert_eq!(r(&d), "");
    }

    #[test]
    fn line_is_space() {
        let d = concat([text("hi"), line(), text("there")]);
        assert_eq!(r(&d), "hi there");
    }

    #[test]
    fn hard_line_emits_newline() {
        let d = concat([text("aaa"), hard_line(), text("bbb")]);
        assert_eq!(r(&d), "aaa\nbbb");
    }

    #[test]
    fn atomic_passes_through() {
        let d = unbreakable(text("very_long_token_here"));
        assert_eq!(r(&d), "very_long_token_here");
    }

    #[test]
    fn renderer_emits_only_lf() {
        let d = concat([text("a"), hard_line(), text("b")]);
        assert!(!r(&d).contains('\r'));
    }

    #[test]
    fn prefix_indents_continuation_lines() {
        // First line of inner is NOT prefixed — the caller prepends
        // "> " separately. Continuation lines do receive "> ".
        let inner = concat([text("a"), hard_line(), text("b")]);
        let d = prefix_lines(quote_prefix(), inner);
        assert_eq!(r(&d), "a\n> b");
    }

    #[test]
    fn prefix_blank_form_between_hardlines() {
        // a\n\nb inside Prefix => a\n>\n> b (blank line is bare ">",
        // next content line is "> b").
        let inner = concat([text("a"), hard_line(), hard_line(), text("b")]);
        let d = prefix_lines(quote_prefix(), inner);
        assert_eq!(r(&d), "a\n>\n> b");
    }

    #[test]
    fn prefix_trailing_hardline_does_not_drain() {
        // Trailing HardLine leaves pending=AfterHardLine but no next
        // leaf drains it — the "> " never lands. Avoids the legacy
        // trim_trailing_marker hack.
        let inner = concat([text("a"), hard_line()]);
        let d = prefix_lines(quote_prefix(), inner);
        assert_eq!(r(&d), "a\n");
    }

    #[test]
    fn prefix_composes_with_outer_prefix() {
        // Nested Prefix gives concatenated drains, innermost last.
        let inner = concat([text("a"), hard_line(), text("b")]);
        let with_indent = prefix_lines(indent4_prefix(), inner);
        let with_quote = prefix_lines(quote_prefix(), with_indent);
        // a\n[> + "    "]b => "a\n>     b"
        assert_eq!(r(&with_quote), "a\n>     b");
    }

    #[test]
    fn prefix_blank_indent_collapses_to_empty() {
        // Indent prefix with blank="" leaves blank lines bare —
        // continuation lines do get the 4-space indent.
        let inner = concat([text("a"), hard_line(), hard_line(), text("b")]);
        let d = prefix_lines(indent4_prefix(), inner);
        assert_eq!(r(&d), "a\n\n    b");
    }

    #[test]
    fn prefix_applies_to_text_internal_newlines() {
        // Constructs like FencedCodeBlock pack multi-line bodies into
        // a single Text; the prefix must still appear after each
        // embedded `\n`.
        let inner = unbreakable(text("```\nbody\n```".to_string()));
        let d = prefix_lines(quote_prefix(), inner);
        assert_eq!(r(&d), "```\n> body\n> ```");
    }

    #[test]
    fn prefix_first_line_has_no_drain() {
        // The first leaf inside Prefix renders without any prefix
        // (pending starts None). Caller is responsible for the
        // first-line form if it differs from content.
        let d = prefix_lines(quote_prefix(), text("first"));
        assert_eq!(r(&d), "first");
    }
}
