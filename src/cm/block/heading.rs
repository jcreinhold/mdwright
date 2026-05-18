//! ATX and setext headings (CM §4.2–§4.3).
//!
//! [`HeadingLevel`] is a private-field newtype: only [`HeadingLevel::try_new`]
//! produces one, and only for 1..=6. [`Heading::try_new`] additionally
//! refuses the setext-plus-level-3+ combination — setext underlines
//! exist only for H1 (`===`) and H2 (`---`).

#![allow(dead_code)]
use std::ops::Range;

/// First byte of `s` after skipping spaces, tabs, and line-feeds.
/// `None` iff `s` is whitespace-only.
fn first_significant_byte(s: &str) -> Option<u8> {
    s.bytes().find(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
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
        b'#' | b'>' | b'-' | b'+' | b'*' | b'=' | b'~' | b'`' | b'<' | b'\t' | b'0'..=b'9'
    )
}

/// Heading attribute trailer (pulldown `Tag::Heading::{id, classes, attrs}`).
/// Carried on [`crate::tree::NodeKind::Heading`] when an
/// `{ #id .class key=val }` trailer was recognised on an ATX heading.
///
/// `source_trailer` is the verbatim `{...}` byte slice the parser saw,
/// used by [`HeadingAttrsStyle::Preserve`] to round-trip the trailer
/// exactly. The `id` / `classes` / `attrs` fields are the parsed view
/// the [`HeadingAttrsStyle::Canonicalise`] emit path consumes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingAttrs {
    /// `{#id …}`. Only the first id in source order is kept (pulldown
    /// drops subsequent ids).
    pub id: Option<String>,
    /// `.class` tokens in source order.
    pub classes: Vec<String>,
    /// `key=value` pairs in source order. The value is `None` for a
    /// bare `key` token with no `=`.
    pub attrs: Vec<(String, Option<String>)>,
    /// Source bytes of the `{...}` trailer (including braces). Empty
    /// only when the trailer scanner failed to relocate the braces in
    /// the heading source — the emit path then falls back to a
    /// canonicalised render.
    pub source_trailer: String,
}

impl HeadingAttrs {
    /// Render the trailer in canonical order: `#id`, then classes in
    /// source order, then `key=value` pairs in source order. Values
    /// containing whitespace are double-quoted; values containing a
    /// double quote are double-quoted with embedded `\"` (Pandoc /
    /// python-markdown convention).
    pub(crate) fn canonical_trailer(&self) -> String {
        let mut tokens: Vec<String> = Vec::new();
        if let Some(id) = &self.id {
            tokens.push(format!("#{id}"));
        }
        for class in &self.classes {
            tokens.push(format!(".{class}"));
        }
        for (k, v) in &self.attrs {
            match v {
                Some(v) if v.chars().any(|c| c.is_ascii_whitespace() || c == '"') => {
                    let escaped: String = v
                        .chars()
                        .flat_map(|c| match c {
                            '"' => vec!['\\', '"'],
                            c => vec![c],
                        })
                        .collect();
                    tokens.push(format!("{k}=\"{escaped}\""));
                }
                Some(v) => tokens.push(format!("{k}={v}")),
                None => tokens.push(k.clone()),
            }
        }
        format!("{{{}}}", tokens.join(" "))
    }
}

/// Locate the `{...}` attribute trailer at the end of `raw`. Returns
/// the byte range of the trailer (braces included) relative to `raw`.
/// Used by the tree builder when pulldown reports a heading with
/// non-empty `id`/`classes`/`attrs`.
///
/// The scanner walks backwards from the last non-whitespace byte of
/// `raw`, expecting `}`. If found, brace-balance backwards (treating
/// nested `{...}` as part of the trailer payload, though pulldown
/// disallows nesting in practice) to the matching `{`. Returns `None`
/// for shapes that don't match — the caller falls back to emitting no
/// trailer.
pub(crate) fn find_attr_trailer_range(raw: &str) -> Option<Range<usize>> {
    let bytes = raw.as_bytes();
    let mut end = bytes.len();
    while end > 0 && matches!(bytes.get(end.saturating_sub(1)), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        end = end.saturating_sub(1);
    }
    if end == 0 || bytes.get(end.saturating_sub(1)) != Some(&b'}') {
        return None;
    }
    let close = end.saturating_sub(1);
    let mut depth = 1i32;
    let mut i = close;
    while i > 0 {
        i = i.saturating_sub(1);
        match bytes.get(i) {
            Some(b'}') => depth = depth.saturating_add(1),
            Some(b'{') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i..end);
                }
            }
            _ => {}
        }
    }
    None
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Heading {
    level: HeadingLevel,
    style: HeadingStyle,
    /// `Some` when the source carried an ATX `{ #id .class }` trailer
    /// (pulldown `Tag::Heading::id|classes|attrs` non-empty); `None`
    /// otherwise.
    attrs: Option<HeadingAttrs>,
}

impl Heading {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn try_new(level: HeadingLevel, style: HeadingStyle) -> Result<Self, HeadingError> {
        if matches!(style, HeadingStyle::Setext) && level.as_u8() > 2 {
            return Err(HeadingError::SetextLevelTooHigh { level: level.as_u8() });
        }
        Ok(Self {
            level,
            style,
            attrs: None,
        })
    }

    /// Builder: attach an attribute trailer. Returns the receiver for
    /// chaining.
    pub(crate) fn with_attrs(mut self, attrs: Option<HeadingAttrs>) -> Self {
        self.attrs = attrs;
        self
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
    fn canonical_trailer_id_then_classes_then_attrs() {
        let attrs = HeadingAttrs {
            id: Some("section".to_owned()),
            classes: vec!["warn".to_owned(), "imp".to_owned()],
            attrs: vec![("data-x".to_owned(), Some("1".to_owned())), ("flag".to_owned(), None)],
            source_trailer: String::new(),
        };
        assert_eq!(attrs.canonical_trailer(), "{#section .warn .imp data-x=1 flag}");
    }

    #[test]
    fn canonical_trailer_quotes_value_with_whitespace() {
        let attrs = HeadingAttrs {
            id: None,
            classes: Vec::new(),
            attrs: vec![("title".to_owned(), Some("hello world".to_owned()))],
            source_trailer: String::new(),
        };
        assert_eq!(attrs.canonical_trailer(), "{title=\"hello world\"}");
    }

    #[test]
    fn canonical_trailer_omits_missing_id_and_empty_lists() {
        let attrs = HeadingAttrs {
            id: None,
            classes: vec!["only".to_owned()],
            attrs: Vec::new(),
            source_trailer: String::new(),
        };
        assert_eq!(attrs.canonical_trailer(), "{.only}");
    }

    #[test]
    fn find_attr_trailer_matches_simple_atx() {
        let raw = "# Heading {#id .class}\n";
        let r = find_attr_trailer_range(raw).expect("trailer present");
        assert_eq!(&raw[r], "{#id .class}");
    }

    #[test]
    fn find_attr_trailer_returns_none_for_plain_heading() {
        assert!(find_attr_trailer_range("# Heading\n").is_none());
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
