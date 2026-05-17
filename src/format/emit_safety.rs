//! Structural emit-safety: validate that the bytes a typed-IR's
//! `pretty()` is about to emit will, when reparsed, produce the same
//! IR shape pulldown gave us for the source span.
//!
//! Every emit decision the formatter makes (delimiter style, escape
//! placement, list marker, fence length, ...) is a guess that depends
//! on context the local `pretty()` doesn't see. Until this module
//! lands the guess was unchecked: when the rewrite changed pulldown's
//! parse decision, the output's HTML diverged from the source's.
//! Bug class A (`_*_` → `***`, `_…*…_` flanking flips, the
//! emphasis-style normalisation fuzz family) is exactly this shape.
//!
//! The fix is a fallback ladder, run once per ambiguous emit site:
//!
//! 1. Try the canonical style.
//! 2. If reparse disagrees, try escaping body bytes that became
//!    syntax-active under the new delimiter.
//! 3. If reparse still disagrees, fall back to source bytes
//!    byte-for-byte.
//!
//! Cost: a reparse per ambiguous site. For emphasis, the "ambiguous
//! site" predicate filters out most runs (a body containing neither
//! `*` nor `_` cannot create the divergence), so the per-document
//! reparse count is bounded by the count of emphasis runs whose body
//! contains the rewritten delimiter character.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::cm::inline::emphasis::EmphasisDelim;
use crate::format::doc::{Doc, RenderOptions, concat, render, text};

/// Surrounding bytes that should flank `wrapped` during the validation
/// reparse. Pulldown's emphasis-flanking rule (CM §6.2) inspects the
/// single character on each side of an opening / closing run; passing
/// the source neighbours lets `parses_as_single_run` reach the same
/// decision pulldown will reach when the emission is embedded in the
/// real document, not in isolation.
///
/// `left` is the byte immediately before the run's source range; `right`
/// is the byte immediately after. `None` means "start / end of document"
/// (or "neighbour not available"), which the predicate treats as
/// whitespace — pulldown's neutral case.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct FlankCtx<'a> {
    pub left: Option<&'a str>,
    pub right: Option<&'a str>,
}

/// Does pulldown, given `wrapped` bytes embedded in `ctx` neighbours,
/// parse them as a single top-level emphasis or strong run? When
/// `false`, the caller's choice of delimiter would let pulldown
/// re-segment the bytes differently from the source's IR.
///
/// The check is structural — it does NOT compare body content, only
/// that exactly one outer run of the requested kind opens at the
/// start, closes at the end, and is the only top-level sibling. The
/// body's correctness is the IR's responsibility; this function
/// guards only the wrapping decision.
pub(crate) fn parses_as_single_run(wrapped: &str, kind: RunKind, ctx: FlankCtx<'_>) -> bool {
    let left = ctx.left.unwrap_or("");
    let right = ctx.right.unwrap_or("");
    let embedded;
    let input = if left.is_empty() && right.is_empty() {
        wrapped
    } else {
        embedded = format!("{left}{wrapped}{right}");
        &embedded
    };
    parses_with_outer_run_at(input, kind, left.len(), wrapped.len())
}

fn parses_with_outer_run_at(input: &str, kind: RunKind, skip_left: usize, run_len: usize) -> bool {
    use pulldown_cmark::OffsetIter;

    let (open_tag, close_tag) = kind.tags();
    let parser: OffsetIter<'_> = Parser::new_ext(input, Options::ENABLE_STRIKETHROUGH).into_offset_iter();

    let target_open = skip_left;
    let target_close = skip_left.saturating_add(run_len);

    // `pre_wrap_depth` tracks Emphasis/Strong/Link/Image containers
    // already open at the moment a candidate `Start(target_kind)`
    // fires. If any are open, the candidate is *nested* inside an
    // outer wrap that the wider input bytes formed — accepting it
    // would lie about the wrap's top-levelness in the real document.
    // See `fuzz_emphasis_block_outer_pair.in` for the bug class.
    //
    // `depth` (the post-`found_open` counter) keeps its old meaning:
    // nested-depth inside the target so we know which `End` closes
    // the run we're matching.
    let mut pre_wrap_depth: u32 = 0;
    let mut depth: u32 = 0;
    let mut found_open = false;
    for (ev, range) in parser {
        match ev {
            Event::Start(ref t)
                if !found_open
                    && range.start == target_open
                    && pre_wrap_depth == 0
                    && std::mem::discriminant(t) == std::mem::discriminant(&open_tag) =>
            {
                found_open = true;
            }
            Event::End(ref t)
                if found_open
                    && depth == 0
                    && range.end == target_close
                    && std::mem::discriminant(t) == std::mem::discriminant(&close_tag) =>
            {
                return true;
            }
            Event::Start(ref t) if !found_open => {
                if is_inline_wrap_start(t) {
                    pre_wrap_depth = pre_wrap_depth.saturating_add(1);
                }
            }
            Event::End(t) if !found_open => {
                if is_inline_wrap_end(t) {
                    pre_wrap_depth = pre_wrap_depth.saturating_sub(1);
                }
            }
            Event::Start(_) if found_open => depth = depth.saturating_add(1),
            Event::End(_) if found_open => depth = depth.saturating_sub(1),
            Event::Start(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::FootnoteReference(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_) => {}
        }
    }
    false
}

fn is_inline_wrap_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Emphasis | Tag::Strong | Tag::Link { .. } | Tag::Image { .. }
    )
}

fn is_inline_wrap_end(tag: TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Link | TagEnd::Image
    )
}

/// Legacy isolation-only validation — kept only for the unit tests in
/// this module, which exercise paths that don't need neighbour
/// context. Production callers should pass real flanking context via
/// [`parses_as_single_run`].
#[cfg(test)]
fn parses_as_single_run_isolated(wrapped: &str, kind: RunKind) -> bool {
    let mut events = Parser::new_ext(wrapped, Options::ENABLE_STRIKETHROUGH);

    let (open_tag, close_tag) = kind.tags();

    // Skip the synthetic paragraph wrapper pulldown adds around
    // top-level inline content.
    let mut first: Option<Event<'_>> = None;
    for ev in &mut events {
        if matches!(ev, Event::Start(Tag::Paragraph)) {
            continue;
        }
        first = Some(ev);
        break;
    }
    let Some(first) = first else {
        return false;
    };
    if !matches!(first, Event::Start(ref t) if std::mem::discriminant(t) == std::mem::discriminant(&open_tag)) {
        return false;
    }

    // Walk the outer run's children, tracking depth so nested
    // emphasis/strong/link bodies don't trigger an early close.
    let mut depth: u32 = 0;
    let mut closed_outer = false;
    for ev in &mut events {
        match ev {
            Event::Start(_) => depth = depth.saturating_add(1),
            Event::End(ref t) if depth == 0 && std::mem::discriminant(t) == std::mem::discriminant(&close_tag) => {
                closed_outer = true;
                break;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Text(_)
            | Event::Code(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::Rule
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_)
            | Event::InlineHtml(_)
            | Event::Html(_) => {}
        }
    }
    if !closed_outer {
        return false;
    }
    // Trailing /Paragraph is fine; anything else (a second top-level
    // run, a stray text event) means the wrapping leaked into
    // adjacent constructs.
    events.all(|ev| matches!(ev, Event::End(TagEnd::Paragraph)))
}

/// Which wrapping the caller asked us to validate.
#[derive(Copy, Clone, Debug)]
pub(crate) enum RunKind {
    Emphasis,
    Strong,
}

impl RunKind {
    fn tags(self) -> (Tag<'static>, TagEnd) {
        match self {
            Self::Emphasis => (Tag::Emphasis, TagEnd::Emphasis),
            Self::Strong => (Tag::Strong, TagEnd::Strong),
        }
    }

    fn wrap_str(self, delim: EmphasisDelim) -> &'static str {
        match (self, delim) {
            (Self::Emphasis, EmphasisDelim::Asterisk) => "*",
            (Self::Emphasis, EmphasisDelim::Underscore) => "_",
            (Self::Strong, EmphasisDelim::Asterisk) => "**",
            (Self::Strong, EmphasisDelim::Underscore) => "__",
        }
    }
}

/// Body bytes that would become flanking-active under `delim`. Used
/// by emphasis emit's fallback ladder before reaching for source
/// verbatim. Returns the escaped body when at least one byte would
/// change parser behaviour; returns `None` when no occurrence of the
/// target byte exists (the caller can skip the second-ladder retry).
pub(crate) fn escape_body_for_emphasis(body: &str, delim: EmphasisDelim) -> Option<String> {
    let target_byte = match delim {
        EmphasisDelim::Asterisk => b'*',
        EmphasisDelim::Underscore => b'_',
    };
    if !body.as_bytes().contains(&target_byte) {
        return None;
    }
    let mut out = String::with_capacity(body.len().saturating_add(4));
    let mut last = 0usize;
    for (i, b) in body.bytes().enumerate() {
        if b == target_byte {
            // Safe slice: target_byte is ASCII, so it always sits
            // on a UTF-8 boundary.
            out.push_str(&body[last..i]);
            out.push('\\');
            out.push(b as char);
            last = i.saturating_add(1);
        }
    }
    out.push_str(&body[last..]);
    Some(out)
}

/// Emit an emphasis or strong run with structural safety. Tries the
/// canonical delimiter; if reparse would disagree, escapes body
/// bytes; if that still disagrees, falls back to the verbatim source
/// slice. The returned `Doc` is guaranteed to reparse to a single
/// run of the requested kind (the body's content equality is the
/// IR's job, not this function's).
pub(crate) fn emit_emphasis_safely<'a>(
    body_doc: Doc<'a>,
    delim: EmphasisDelim,
    kind: RunKind,
    source_slice: &str,
    flank: FlankCtx<'_>,
) -> Doc<'a> {
    let body_str = render(&body_doc, &RenderOptions);
    let wrapping = kind.wrap_str(delim);

    let candidate = format!("{wrapping}{body_str}{wrapping}");
    if parses_as_single_run(&candidate, kind, flank) {
        return wrap_in_delim(body_doc, kind, delim);
    }

    if let Some(escaped) = escape_body_for_emphasis(&body_str, delim) {
        let cand2 = format!("{wrapping}{escaped}{wrapping}");
        if parses_as_single_run(&cand2, kind, flank) {
            // Escaping rewrote body bytes; the original Doc is
            // discarded in favour of the escaped string. Structural
            // attributes (Atomic / Prefix) inside the body don't
            // survive — acceptable because escaping has to operate
            // at byte level.
            return wrap_in_delim(text(escaped), kind, delim);
        }
    }

    // The other delimiter, with the same escape-or-not logic. This
    // catches cases where the configured style + flanking neighbours
    // conspire against emission, but the *other* style would be
    // safe (`foo***bar***baz` is the canonical example: the inner
    // strong collision flips the outer emphasis from `*` to `_`, but
    // `foo_..._baz` is intraword underscore. The unflipped `*` would
    // work here).
    let other = delim.flip();
    let other_wrap = kind.wrap_str(other);
    let candidate3 = format!("{other_wrap}{body_str}{other_wrap}");
    if parses_as_single_run(&candidate3, kind, flank) {
        return wrap_in_delim(body_doc, kind, other);
    }
    if let Some(escaped) = escape_body_for_emphasis(&body_str, other) {
        let cand4 = format!("{other_wrap}{escaped}{other_wrap}");
        if parses_as_single_run(&cand4, kind, flank) {
            return wrap_in_delim(text(escaped), kind, other);
        }
    }

    // Source verbatim: keeps the original delimiter and body bytes.
    text(source_slice.to_owned())
}

fn wrap_in_delim(body: Doc<'_>, kind: RunKind, delim: EmphasisDelim) -> Doc<'_> {
    let d = kind.wrap_str(delim);
    concat([text(d), body, text(d)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_wrap_around_literal_star_rejected() {
        // bug H: `_*_` source produces body=`*`, delim=Asterisk →
        // `***`. Pulldown sees three literal asterisks, no emphasis.
        assert!(!parses_as_single_run_isolated("***", RunKind::Emphasis));
    }

    #[test]
    fn star_wrap_around_plain_text_accepted() {
        assert!(parses_as_single_run_isolated("*hello*", RunKind::Emphasis));
    }

    #[test]
    fn underscore_wrap_around_literal_star_accepted() {
        assert!(parses_as_single_run_isolated("_*_", RunKind::Emphasis));
    }

    #[test]
    fn star_wrap_around_escaped_star_accepted() {
        assert!(parses_as_single_run_isolated(r"*\**", RunKind::Emphasis));
    }

    #[test]
    fn strong_double_wrap_around_literal_star_rejected() {
        assert!(!parses_as_single_run_isolated("*****", RunKind::Strong));
    }

    #[test]
    fn strong_double_wrap_around_plain_text_accepted() {
        assert!(parses_as_single_run_isolated("**hi**", RunKind::Strong));
    }

    #[test]
    fn nested_emphasis_in_emphasis_accepted() {
        // GFM spec example 378: *(*foo*)* is nested emphasis. The
        // outer wrap must survive even though the body contains
        // another emphasis run.
        assert!(parses_as_single_run_isolated("*(*foo*)*", RunKind::Emphasis));
    }

    #[test]
    fn escape_body_inserts_backslash_before_asterisk() {
        let out = escape_body_for_emphasis("a*b", EmphasisDelim::Asterisk);
        assert_eq!(out.as_deref(), Some(r"a\*b"));
    }

    #[test]
    fn escape_body_no_op_when_target_absent() {
        assert_eq!(escape_body_for_emphasis("plain", EmphasisDelim::Asterisk), None);
    }

    #[test]
    fn escape_body_handles_non_ascii() {
        let out = escape_body_for_emphasis("αβ*γ", EmphasisDelim::Asterisk);
        assert_eq!(out.as_deref(), Some(r"αβ\*γ"));
    }

    #[test]
    fn escape_then_wrap_reparses_correctly() {
        let escaped = escape_body_for_emphasis("a*b", EmphasisDelim::Asterisk).unwrap_or_default();
        assert_eq!(escaped, r"a\*b");
        let wrapped = format!("*{escaped}*");
        assert!(parses_as_single_run_isolated(&wrapped, RunKind::Emphasis));
    }
}
