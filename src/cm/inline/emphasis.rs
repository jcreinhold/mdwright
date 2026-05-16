//! Typed emphasis / strong values.
//!
//! [`EmphasisRun`] and [`StrongRun`] capture the source delimiter byte
//! pulldown saw (`*` or `_`) at parse time and expose one method,
//! `resolve`, that decides the final delimiter byte from four
//! previously-braided concerns: the source byte, the configured italic
//! style, the resolved delimiter of the most recent same-kind sibling
//! (collision flip), and the resolved delimiter of the first child
//! (nested-fusion flip).
//!
//! The IR-builder layer ([`crate::tree::TreeBuilder`]) is style-agnostic
//! — `Ir::parse` runs once and may feed multiple formatter passes with
//! different `FmtOptions`. So the typed value stores only the parse-time
//! datum (`source_delim`); `resolve` runs in the format walker but is
//! the sole site that decides the delimiter, replacing four braided
//! helpers that previously lived in `src/format/inline.rs`.
//!
//! Pulldown only emits an emphasis event when at least one of `*` / `_`
//! is admissible by CM §6.2 in the source position, so `resolve` always
//! returns a delimiter; there is no irreducible-collision case.

use crate::config::ItalicStyle;

/// Resolved delimiter byte for an emphasis or strong run.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum EmphasisDelim {
    Asterisk,
    Underscore,
}

impl EmphasisDelim {
    pub(crate) fn flip(self) -> Self {
        match self {
            Self::Asterisk => Self::Underscore,
            Self::Underscore => Self::Asterisk,
        }
    }

    /// `"*"` / `"_"` for emphasis; the strong renderer doubles this.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Asterisk => "*",
            Self::Underscore => "_",
        }
    }
}

/// Typed emphasis run. Carries the source delimiter byte pulldown saw
/// at the opening run; body content lives in the tree as children of
/// the enclosing `NodeKind::Emphasis(_)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct EmphasisRun {
    source_delim: u8,
}

/// Typed strong run. Same shape as [`EmphasisRun`]; `**` vs `__` is
/// chosen with the same rules. `as_strong_str` doubles the byte.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StrongRun {
    source_delim: u8,
}

/// Format-time context for [`EmphasisRun::resolve`] / [`StrongRun::resolve`].
#[derive(Copy, Clone, Debug)]
pub(crate) struct ResolveCtx {
    /// Configured italic style from `FmtOptions`.
    pub style: ItalicStyle,
    /// Resolved delim of the most recent already-rendered sibling of
    /// the same kind. `None` if the previous sibling is not Emphasis
    /// (for `EmphasisRun::resolve`) or not Strong (for `StrongRun`).
    pub left_sibling_delim: Option<EmphasisDelim>,
    /// `Some(d)` if this run's first child resolves to delimiter `d`.
    /// Forces the outer run to flip so the delimiters do not fuse
    /// (e.g. `*` outside, `**` inside, never `***x***`).
    pub first_child_delim: Option<EmphasisDelim>,
}

impl EmphasisRun {
    pub(crate) fn from_source(source_delim: u8) -> Self {
        Self { source_delim }
    }

    pub(crate) fn resolve(self, ctx: ResolveCtx) -> EmphasisDelim {
        resolve(self.source_delim, ctx)
    }
}

impl StrongRun {
    pub(crate) fn from_source(source_delim: u8) -> Self {
        Self { source_delim }
    }

    pub(crate) fn resolve(self, ctx: ResolveCtx) -> EmphasisDelim {
        resolve(self.source_delim, ctx)
    }
}

fn resolve(source_delim: u8, ctx: ResolveCtx) -> EmphasisDelim {
    let mut delim = resolve_initial(source_delim, ctx.style);
    if ctx.first_child_delim == Some(delim) {
        delim = delim.flip();
    }
    if ctx.left_sibling_delim == Some(delim) {
        delim = delim.flip();
    }
    delim
}

fn resolve_initial(source_delim: u8, style: ItalicStyle) -> EmphasisDelim {
    match style {
        ItalicStyle::Asterisk => EmphasisDelim::Asterisk,
        ItalicStyle::Underscore => EmphasisDelim::Underscore,
        ItalicStyle::Preserve => {
            if source_delim == b'_' {
                EmphasisDelim::Underscore
            } else {
                EmphasisDelim::Asterisk
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(
        style: ItalicStyle,
        left: Option<EmphasisDelim>,
        child: Option<EmphasisDelim>,
    ) -> ResolveCtx {
        ResolveCtx {
            style,
            left_sibling_delim: left,
            first_child_delim: child,
        }
    }

    #[test]
    fn preserve_keeps_source_asterisk() {
        let run = EmphasisRun::from_source(b'*');
        assert_eq!(
            run.resolve(ctx(ItalicStyle::Preserve, None, None)),
            EmphasisDelim::Asterisk
        );
    }

    #[test]
    fn preserve_keeps_source_underscore() {
        let run = EmphasisRun::from_source(b'_');
        assert_eq!(
            run.resolve(ctx(ItalicStyle::Preserve, None, None)),
            EmphasisDelim::Underscore
        );
    }

    #[test]
    fn asterisk_style_rewrites_underscore() {
        let run = EmphasisRun::from_source(b'_');
        assert_eq!(
            run.resolve(ctx(ItalicStyle::Asterisk, None, None)),
            EmphasisDelim::Asterisk
        );
    }

    #[test]
    fn underscore_style_rewrites_asterisk() {
        let run = EmphasisRun::from_source(b'*');
        assert_eq!(
            run.resolve(ctx(ItalicStyle::Underscore, None, None)),
            EmphasisDelim::Underscore
        );
    }

    #[test]
    fn sibling_collision_flips() {
        let run = EmphasisRun::from_source(b'*');
        let d = run.resolve(ctx(
            ItalicStyle::Asterisk,
            Some(EmphasisDelim::Asterisk),
            None,
        ));
        assert_eq!(d, EmphasisDelim::Underscore);
    }

    #[test]
    fn nested_child_collision_flips() {
        let run = EmphasisRun::from_source(b'*');
        let d = run.resolve(ctx(
            ItalicStyle::Asterisk,
            None,
            Some(EmphasisDelim::Asterisk),
        ));
        assert_eq!(d, EmphasisDelim::Underscore);
    }

    #[test]
    fn child_flip_then_sibling_flip_stack() {
        // initial=*; child=* forces flip to _; sibling=_ forces flip
        // back to *.
        let run = EmphasisRun::from_source(b'*');
        let d = run.resolve(ctx(
            ItalicStyle::Asterisk,
            Some(EmphasisDelim::Underscore),
            Some(EmphasisDelim::Asterisk),
        ));
        assert_eq!(d, EmphasisDelim::Asterisk);
    }

    #[test]
    fn strong_uses_same_logic() {
        let run = StrongRun::from_source(b'_');
        assert_eq!(
            run.resolve(ctx(ItalicStyle::Asterisk, None, None)),
            EmphasisDelim::Asterisk
        );
    }

    #[test]
    fn flip_is_involutive() {
        assert_eq!(
            EmphasisDelim::Asterisk.flip().flip(),
            EmphasisDelim::Asterisk
        );
        assert_eq!(
            EmphasisDelim::Underscore.flip().flip(),
            EmphasisDelim::Underscore
        );
    }
}
