//! The `Doc` combinator IR and its renderer.
//!
//! A `Doc` is a tree of layout instructions. The wrap pass
//! ([`crate::format::wrap`]) makes all break decisions before the
//! renderer runs; the renderer then walks the tree once and emits a
//! string.
//!
//! ## Semantics
//!
//! - [`Doc::Text(s)`](Doc::Text) — literal text; must not contain `\n`.
//! - [`Doc::Line`] — soft break. Renders as a single space; the wrap
//!   pass converts the soft breaks it chose to keep as line breaks
//!   into [`Doc::HardLine`] before render time, so any `Doc::Line`
//!   left at render time is intentionally flat.
//! - [`Doc::HardLine`] — newline followed by the configured indent.
//! - [`Doc::Atomic`] — atomic box; the wrap pass refuses to split
//!   its contents across lines. Used for inline code, URLs,
//!   autolinks, raw HTML — anything whose syntax breaks if split.
//!   The renderer just recurses into it.
//! - [`Doc::Concat`] — render children in order.

use std::borrow::Cow;

/// Tree of layout instructions; see the module docs.
#[derive(Clone, Debug)]
pub(crate) enum Doc<'a> {
    Text(Cow<'a, str>),
    Line,
    HardLine,
    Atomic(Box<Self>),
    Concat(Box<[Self]>),
}

/// Knobs for [`render`]. Currently empty: layout decisions live in
/// the wrap pass and the renderer is parameter-free. Kept as a
/// distinct type so future width- or indent-sensitive renderers do
/// not change the call signature.
#[derive(Clone, Debug, Default)]
pub(crate) struct RenderOptions;

// --- constructors ---------------------------------------------------

pub(crate) fn text<'a>(s: impl Into<Cow<'a, str>>) -> Doc<'a> {
    Doc::Text(s.into())
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

// --- renderer -------------------------------------------------------

/// Render `doc` into a `String` using `opts`. Soft `Doc::Line`
/// markers render as a single space; hard breaks render as a bare
/// newline. The wrap pass is expected to have converted any soft
/// break it intended to honour into `Doc::HardLine` before this
/// point. Block-level indentation (lists, blockquotes) is handled
/// by the block serializer post-processing the rendered string, not
/// by the renderer.
pub(crate) fn render(doc: &Doc<'_>, _opts: &RenderOptions) -> String {
    let mut out = String::new();
    let mut stack: Vec<&Doc<'_>> = Vec::with_capacity(16);
    stack.push(doc);

    while let Some(d) = stack.pop() {
        match d {
            Doc::Text(s) => out.push_str(s),
            Doc::Line => out.push(' '),
            Doc::HardLine => out.push('\n'),
            Doc::Atomic(inner) => stack.push(inner),
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    stack.push(item);
                }
            }
        }
    }

    out
}

// --- tests ----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{Doc, RenderOptions, concat, hard_line, line, render, text, unbreakable};

    fn r(doc: &Doc<'_>) -> String {
        render(doc, &RenderOptions)
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
}
