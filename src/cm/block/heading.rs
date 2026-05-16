//! ATX and setext headings (CM §4.2–§4.3).
//!
//! [`HeadingLevel`] is a private-field newtype: only [`HeadingLevel::try_new`]
//! produces one, and only for 1..=6. [`Heading::try_new`] additionally
//! refuses the setext-plus-level-3+ combination — setext underlines
//! exist only for H1 (`===`) and H2 (`---`).

use crate::format::doc::{Doc, concat, hard_line, text, unbreakable};
use crate::format::pretty::PrettyCtx;
use crate::tree::NodeId;

/// First byte of `s` after skipping spaces, tabs, and line-feeds.
/// `None` iff `s` is whitespace-only.
fn first_significant_byte(s: &str) -> Option<u8> {
    s.bytes()
        .find(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
}

/// Split a setext heading's source bytes into (body, underline). The
/// raw range from pulldown always has the shape `body LF underline
/// [LF]`: trim a trailing `\n` (which pulldown sometimes includes),
/// then split at the last remaining `\n`. Returns `None` if the shape
/// is not present (defensive: the caller falls back to ATX).
fn split_setext_source(raw: &str) -> Option<(&str, &str)> {
    let trimmed = raw.trim_end_matches('\n');
    let last_nl = trimmed.rfind('\n')?;
    let body = trimmed.get(..last_nl)?;
    let underline = trimmed.get(last_nl.saturating_add(1)..)?;
    Some((body, underline))
}

/// `true` iff a setext heading with body `body_source` can re-parse as
/// itself. The check is a pure function of source bytes (no inline
/// `Doc` walk) so the setext-vs-ATX decision is stable across re-parse
/// — fuzz-found bug class: a body whose rendered `Doc` carried
/// `HardLine`/`Line` markers (e.g. from control-byte handling) made
/// pass 1 keep setext and pass 2 flip to ATX. Source bytes survive
/// formatting identically; rendered `Doc` does not.
///
/// Setext is safe iff:
/// - body has at least one significant byte, AND
/// - **either** the body is multi-line (`\n` inside body source — the
///   setext shape binds it as heading regardless of what individual
///   lines look like) **or** the first significant byte is not a CM
///   block-leader (`#`/`>`/`-`/`+`/`*`/`=`/`~`/backtick/`<`/tab/digit
///   — those would re-parse a single-line body as a different block,
///   splitting the heading).
fn setext_body_safe(body_source: &str) -> bool {
    let Some(first) = first_significant_byte(body_source) else {
        return false;
    };
    if body_source.contains('\n') {
        return true;
    }
    !matches!(
        first,
        b'#' | b'>'
            | b'-'
            | b'+'
            | b'*'
            | b'='
            | b'~'
            | b'`'
            | b'<'
            | b'\t'
            | b'0'..=b'9'
    )
}

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
            // The setext-vs-ATX decision and the rendered body both
            // come from source bytes here. Two reasons:
            //   (a) the decision predicate must be a pure function of
            //       inputs that re-parse identically — the rendered
            //       inline `Doc`'s `HardLine`/`Line` placement shifts
            //       between passes when control bytes or escape
            //       choices change, so it cannot drive the decision;
            //   (b) the emitted body must preserve the source's
            //       line structure so the next parse classifies the
            //       heading the same way. The rendered `Doc` joins
            //       soft breaks as spaces under default options,
            //       collapsing a multi-line body to one line — pass 2
            //       would then see a single-line setext body whose
            //       first byte triggers ATX. Source verbatim emit
            //       breaks that cycle.
            // Inline normalisations (italic style, etc.) do not apply
            // inside setext bodies; the trade-off is acceptable —
            // setext bodies are typically plain text and idempotence
            // is the load-bearing invariant.
            let raw = ctx
                .tree
                .node(id)
                .map(|n| ctx.source.get(n.raw_range.clone()).unwrap_or(""))
                .unwrap_or("");
            if let Some((body_source, _underline_source)) = split_setext_source(raw)
                && setext_body_safe(body_source)
            {
                // `unbreakable` keeps the body source's embedded `\n`
                // bytes from being split by the wrap pass (which
                // treats `\n` as ASCII whitespace, joining lines with
                // spaces and collapsing a multi-line setext body to a
                // single line — exactly the bug this fix targets).
                let body_doc = unbreakable(text(body_source.to_owned()));
                let first_line_width = body_source
                    .lines()
                    .next()
                    .map_or(3, |l| l.chars().count())
                    .max(3);
                let underline_char = if level == 1 { '=' } else { '-' };
                let underline: String =
                    std::iter::repeat_n(underline_char, first_line_width).collect();
                drop(body);
                return concat([body_doc, hard_line(), text(underline), hard_line()]);
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
