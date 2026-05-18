//! Delimiter classification, recogniser error types, and per-region
//! tagged spans.
//!
//! The recogniser ([`super::scan::scan_math_regions`]) produces one
//! [`super::MathRegion`] per recognised math region, each tagged with
//! a [`MathSpan`] that records *which* delimiter family or environment
//! introduced it plus the body byte range. The pretty-printer
//! ([`super::pretty`]) dispatches on the span variant.
//!
//! Unmatched openers and brace-imbalanced bodies become [`MathError`]
//! values so the lint rules `math/unbalanced-delim`,
//! `math/unbalanced-env`, and `math/unbalanced-braces` can surface a
//! useful diagnostic without aborting the scan.

#![allow(dead_code)]
use std::borrow::Cow;
use std::ops::Range;

use super::env::EnvKind;

/// One of the four primitive math delimiter families.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnyDelim {
    /// `\(` / `\)`
    Paren,
    /// `\[` / `\]`
    Bracket,
    /// `$` / `$`
    Dollar,
    /// `$$` / `$$`
    Dollar2,
}

impl AnyDelim {
    pub const fn is_display(self) -> bool {
        matches!(self, Self::Bracket | Self::Dollar2)
    }

    pub const fn open(self) -> &'static str {
        match self {
            Self::Paren => r"\(",
            Self::Bracket => r"\[",
            Self::Dollar => "$",
            Self::Dollar2 => "$$",
        }
    }

    pub const fn close(self) -> &'static str {
        match self {
            Self::Paren => r"\)",
            Self::Bracket => r"\]",
            Self::Dollar => "$",
            Self::Dollar2 => "$$",
        }
    }
}

/// Inline delimiter pair carried on [`MathSpan::Inline`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum InlineDelim {
    /// `\(` / `\)`
    Paren,
    /// `$` / `$`
    Dollar,
}

impl InlineDelim {
    pub(crate) const fn open(self) -> &'static str {
        match self {
            Self::Paren => r"\(",
            Self::Dollar => "$",
        }
    }

    pub(crate) const fn close(self) -> &'static str {
        match self {
            Self::Paren => r"\)",
            Self::Dollar => "$",
        }
    }
}

/// Display delimiter pair carried on [`MathSpan::Display`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DisplayDelim {
    /// `\[` / `\]`
    Bracket,
    /// `$$` / `$$`
    Dollar2,
}

impl DisplayDelim {
    pub(crate) const fn open(self) -> &'static str {
        match self {
            Self::Bracket => r"\[",
            Self::Dollar2 => "$$",
        }
    }

    pub(crate) const fn close(self) -> &'static str {
        match self {
            Self::Bracket => r"\]",
            Self::Dollar2 => "$$",
        }
    }
}

/// Per-region classification produced by the scanner.
///
/// Each variant carries the body as a [`MathBody`] — a hidden
/// abstraction that yields clean math content regardless of where the
/// math appeared in the source (top-level, blockquote, list item).
/// The pretty-printer dispatches on the variant and reads the body
/// via [`MathBody::as_str`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MathSpan {
    Inline { delim: InlineDelim, body: MathBody },
    Display { delim: DisplayDelim, body: MathBody },
    Environment { env: EnvKind, body: MathBody },
}

impl MathSpan {
    /// Body of this span. Provided so callers do not have to
    /// destructure the enum to read the body.
    pub(crate) fn body(&self) -> &MathBody {
        match self {
            Self::Inline { body, .. } | Self::Display { body, .. } | Self::Environment { body, .. } => body,
        }
    }
}

/// Math-body content with container prefixes hidden.
///
/// `range` is the outer body byte range (between the delimiters, in
/// source bytes). `transparent` lists byte ranges intersecting the
/// body that the consumer should treat as if they do not exist —
/// blockquote `>` markers and list-item continuation indentation
/// captured by the recogniser at scan time.
///
/// The abstraction lets callers consume math content without knowing
/// whether the region happened to be nested in a container. The
/// common case (no container) keeps the [`Cow::Borrowed`] fast path;
/// container-nested math allocates one `String` per region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathBody {
    range: Range<usize>,
    /// Sorted, non-overlapping ranges that intersect `range`. Stored
    /// unclipped — `as_str` and `clean_offset_to_source` clip against
    /// `range` on every use.
    transparent: Box<[Range<usize>]>,
}

impl MathBody {
    pub(crate) fn new(range: Range<usize>, transparent: Box<[Range<usize>]>) -> Self {
        Self { range, transparent }
    }

    /// `true` iff the body has any transparent (container-prefix)
    /// runs. The pretty-printer uses this as a fast probe: container-
    /// nested math is rendered structurally so the surrounding
    /// `Doc::Prefix` cascade can re-apply the prefixes; the
    /// alternative line-standalone byte check only matters when the
    /// math is at the document root.
    pub(crate) fn has_transparent_runs(&self) -> bool {
        !self.transparent.is_empty()
    }

    /// Materialised body content with transparent runs removed.
    /// Borrows the source slice when no runs intersect; allocates a
    /// new `String` only when stripping is required.
    pub(crate) fn as_str<'src>(&self, source: &'src str) -> Cow<'src, str> {
        if self.transparent.is_empty() {
            return Cow::Borrowed(source.get(self.range.clone()).unwrap_or(""));
        }
        let mut out = String::with_capacity(self.range.end.saturating_sub(self.range.start));
        let mut cursor = self.range.start;
        for run in &self.transparent {
            let run_start = run.start.max(self.range.start);
            let run_end = run.end.min(self.range.end);
            if run_start >= run_end {
                continue;
            }
            if cursor < run_start
                && let Some(slice) = source.get(cursor..run_start)
            {
                out.push_str(slice);
            }
            cursor = run_end;
        }
        if cursor < self.range.end
            && let Some(slice) = source.get(cursor..self.range.end)
        {
            out.push_str(slice);
        }
        Cow::Owned(out)
    }

    /// Map a byte offset inside the clean (stripped) body back to a
    /// source-absolute byte. Walks the same prefix iteration
    /// [`Self::as_str`] uses, so an offset produced by a check on the
    /// clean body resolves to the correct source position even when
    /// container prefixes have been stripped.
    pub(crate) fn clean_offset_to_source(&self, clean_off: usize) -> usize {
        if self.transparent.is_empty() {
            return self.range.start.saturating_add(clean_off);
        }
        let mut consumed = 0usize;
        let mut cursor = self.range.start;
        for run in &self.transparent {
            let run_start = run.start.max(self.range.start);
            let run_end = run.end.min(self.range.end);
            if run_start >= run_end {
                continue;
            }
            let slice_len = run_start.saturating_sub(cursor);
            if clean_off < consumed.saturating_add(slice_len) {
                return cursor.saturating_add(clean_off.saturating_sub(consumed));
            }
            consumed = consumed.saturating_add(slice_len);
            cursor = run_end;
        }
        cursor.saturating_add(clean_off.saturating_sub(consumed))
    }
}

/// An unrecoverable shape the recogniser saw. The scanner never
/// panics; it accumulates these and keeps scanning the rest of the
/// document.
//
// The `Unbalanced` prefix is part of the user-facing diagnostic
// vocabulary (it mirrors the rule names `math/unbalanced-delim`,
// `math/unbalanced-env`, `math/unbalanced-braces`), so the
// shared-prefix nudge does not apply here.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug)]
pub enum MathError {
    /// `\[`, `\(`, `$$`, or `$` with no matching close.
    UnbalancedDelim {
        delim: AnyDelim,
        /// Byte range of the opening delimiter token.
        range: Range<usize>,
    },
    /// `\begin{name}` with no matching `\end{name}` at the same depth.
    UnbalancedEnv {
        name: String,
        /// Byte range covering `\begin{name}` itself.
        range: Range<usize>,
    },
    /// `{` and `}` inside a recognised math body do not balance. The
    /// region still scans (markers are balanced); the pretty-printer
    /// falls back to verbatim emission because we cannot safely
    /// normalise content with broken brace nesting.
    UnbalancedBraces {
        /// Byte offset (absolute, into the source) of the offending
        /// brace — either an unmatched `}` or the start of the body
        /// when the document ends mid-group.
        offset: usize,
        /// Byte range of the math region whose body failed validation.
        region: Range<usize>,
    },
}
