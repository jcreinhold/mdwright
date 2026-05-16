//! ATX and setext headings (CM §4.2–§4.3).
//!
//! [`HeadingLevel`] is a private-field newtype: only [`HeadingLevel::try_new`]
//! produces one, and only for 1..=6. [`Heading::try_new`] additionally
//! refuses the setext-plus-level-3+ combination — setext underlines
//! exist only for H1 (`===`) and H2 (`---`).

use crate::format::doc::{Doc, RenderOptions, concat, hard_line, render, text};
use crate::format::pretty::PrettyCtx;
use crate::tree::NodeId;

/// A heading level in 1..=6. Constructed only via [`HeadingLevel::try_new`];
/// the inner byte is intentionally inaccessible so out-of-range values
/// are unrepresentable.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeadingLevel(u8);

impl HeadingLevel {
    pub(crate) fn try_new(n: u8) -> Result<Self, HeadingError> {
        if (1..=6).contains(&n) {
            Ok(Self(n))
        } else {
            Err(HeadingError::InvalidLevel(n))
        }
    }

    pub(crate) fn as_u8(self) -> u8 {
        self.0
    }
}

/// Source-form discriminant. Setext headings (`Foo\n===`) carry an
/// underline; ATX headings (`# Foo`) carry an opening `#` run.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum HeadingStyle {
    Atx,
    Setext,
}

/// A heading whose existence guarantees CM §4.2/§4.3 well-formedness:
/// level ∈ 1..=6, and setext implies level ≤ 2. The inline body lives
/// in the surrounding [`crate::tree::Tree`] arena as direct children
/// of the corresponding `Node`; this value carries only the data that
/// the legacy `NodeKind::Heading { level, setext }` cannot encode as a
/// type-level invariant.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Heading {
    level: HeadingLevel,
    style: HeadingStyle,
}

impl Heading {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn try_new(level: HeadingLevel, style: HeadingStyle) -> Result<Self, HeadingError> {
        if matches!(style, HeadingStyle::Setext) && level.as_u8() > 2 {
            return Err(HeadingError::SetextLevelTooHigh {
                level: level.as_u8(),
            });
        }
        Ok(Self { level, style })
    }

    pub(crate) fn level(self) -> HeadingLevel {
        self.level
    }

    pub(crate) fn style(self) -> HeadingStyle {
        self.style
    }

    /// Emit an ATX (`# Body`) or setext (`Body\n===`) heading. `body`
    /// is the already-rendered inline doc; the dispatcher produces it.
    /// Setext headings carry their underline width at render time so
    /// the line matches the inline's display width (minimum 3).
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: NodeId) -> Doc<'a> {
        let body = crate::format::inline::pretty_inline_children(ctx, id);
        let level = self.level.as_u8();
        if matches!(self.style, HeadingStyle::Setext) && level <= 2 {
            // Setext re-parses correctly only when the body's first
            // line does not look like a different block-starter to
            // pulldown. A body of `*`, for example, would re-parse as
            // a list bullet — the `===` underline then becomes a
            // bare paragraph, breaking idempotence. Walk the body Doc
            // directly so soft breaks (which render as spaces) still
            // count as line breaks for the multi-line check.
            if setext_body_safe(&body) {
                let rendered = render(&body, &RenderOptions);
                let width = rendered
                    .lines()
                    .next()
                    .map_or(3, |l| l.chars().count())
                    .max(3);
                let underline_char = if level == 1 { '=' } else { '-' };
                let underline: String = std::iter::repeat_n(underline_char, width).collect();
                return concat([body, hard_line(), text(underline), hard_line()]);
            }
        }
        let lvl = level.clamp(1, 6) as usize;
        let prefix: String = std::iter::repeat_n('#', lvl).collect::<String>() + " ";
        concat([text(prefix), body, hard_line()])
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum HeadingError {
    InvalidLevel(u8),
    SetextLevelTooHigh { level: u8 },
}

/// Conservative gate for emitting setext form. Walks `body` directly
/// so soft breaks (which render as spaces in isolation but become
/// hard newlines during the wrap pass) still count as line breaks
/// for the multi-line detection.
///
/// Decision matrix:
/// - empty body → ATX (setext requires non-blank content).
/// - multi-line body → setext (ATX cannot carry multi-line content;
///   the wrap pass would split it into a heading + a paragraph).
/// - single-line body whose first non-empty byte is a block-leader
///   (`*`, `-`, `+`, `#`, `>`, `=`, `~`, backtick, `<`, tab, digit)
///   → ATX (setext would re-parse as a different block).
/// - otherwise → setext.
fn setext_body_safe(body: &Doc<'_>) -> bool {
    let mut first_byte: Option<u8> = None;
    let mut has_break = false;
    walk_body(body, &mut first_byte, &mut has_break);
    let Some(first) = first_byte else {
        return false; // empty body
    };
    if has_break {
        // Multi-line body — keep setext; ATX cannot represent it.
        return true;
    }
    match first {
        // ATX heading, blockquote, list bullets, setext underline,
        // fenced code, indented code (tab counts), HTML start.
        b'#' | b'>' | b'-' | b'+' | b'*' | b'=' | b'~' | b'`' | b'<' | b'\t' => false,
        // Digit could be an ordered-list marker if `.` or `)` follows.
        b'0'..=b'9' => false,
        _ => true,
    }
}

/// Iterative pre-order walk: capture the first text byte and note
/// whether any `Doc::Line` or `Doc::HardLine` appears.
fn walk_body(body: &Doc<'_>, first_byte: &mut Option<u8>, has_break: &mut bool) {
    let mut stack: Vec<&Doc<'_>> = vec![body];
    while let Some(node) = stack.pop() {
        if first_byte.is_some() && *has_break {
            return;
        }
        match node {
            Doc::Text(s) => {
                if first_byte.is_none()
                    && let Some(b) = s.as_bytes().iter().find(|&&b| b != b' ' && b != b'\t')
                {
                    *first_byte = Some(*b);
                }
            }
            Doc::Line | Doc::HardLine => *has_break = true,
            Doc::Atomic(inner) | Doc::Prefix(_, inner) => stack.push(inner),
            Doc::Concat(items) => {
                for item in items.iter().rev() {
                    stack.push(item);
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn level_in_range_constructs() {
        for n in 1u8..=6 {
            assert_eq!(HeadingLevel::try_new(n).map(HeadingLevel::as_u8), Ok(n));
        }
    }

    #[test]
    fn level_out_of_range_rejected() {
        assert_eq!(HeadingLevel::try_new(0), Err(HeadingError::InvalidLevel(0)));
        assert_eq!(HeadingLevel::try_new(7), Err(HeadingError::InvalidLevel(7)));
    }

    #[test]
    fn atx_accepts_every_level() {
        for n in 1u8..=6 {
            let lvl = HeadingLevel::try_new(n).expect("range");
            assert!(Heading::try_new(lvl, HeadingStyle::Atx).is_ok());
        }
    }

    #[test]
    fn setext_accepts_only_one_and_two() {
        for n in 1u8..=2 {
            let lvl = HeadingLevel::try_new(n).expect("range");
            assert!(Heading::try_new(lvl, HeadingStyle::Setext).is_ok());
        }
        for n in 3u8..=6 {
            let lvl = HeadingLevel::try_new(n).expect("range");
            assert_eq!(
                Heading::try_new(lvl, HeadingStyle::Setext),
                Err(HeadingError::SetextLevelTooHigh { level: n }),
            );
        }
    }
}
