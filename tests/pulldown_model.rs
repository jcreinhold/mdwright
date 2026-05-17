//! Drift tests for the rules in `docs/architecture/pulldown-model.md`.
//!
//! One test per documented rule. Each feeds the documented example to
//! pulldown directly and asserts the documented event-stream shape.
//! When pulldown's behaviour changes (a version bump, an upstream
//! bug fix), the failing assertion message names this file and the
//! `docs/architecture/pulldown-model.md` section the contributor must
//! update **first**, before changing any mdwright code in response.
//!
//! These tests are deliberately allowed to use `pulldown_cmark::Parser`
//! directly — they are checking pulldown's behaviour, not mdwright's,
//! so the production chokepoint (`src/parse.rs`) is not in scope.

#![allow(clippy::expect_used, clippy::wildcard_enum_match_arm)]

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

const MODEL_DOC: &str = "docs/architecture/pulldown-model.md";

fn opts() -> Options {
    Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS
}

fn collect(s: &str) -> Vec<Event<'_>> {
    Parser::new_ext(s, opts()).collect()
}

/// §1: a single LF inside a paragraph becomes `SoftBreak`; two LFs end
/// the block.
#[test]
fn line_endings_softbreak_between_lines() {
    let events = collect("a\nb\n");
    let kinds: Vec<&'static str> = events
        .iter()
        .map(|e| match e {
            Event::Start(Tag::Paragraph) => "Start(Paragraph)",
            Event::End(TagEnd::Paragraph) => "End(Paragraph)",
            Event::Text(_) => "Text",
            Event::SoftBreak => "SoftBreak",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["Start(Paragraph)", "Text", "SoftBreak", "Text", "End(Paragraph)"],
        "pulldown's prose line-ending rule changed; update {MODEL_DOC} §1 \
         before touching src/format/semantic.rs::canonical_events"
    );
}

/// §2: indented code blocks preserve the terminating newline of the
/// last content line even when a trailing whitespace-only line follows.
#[test]
fn indented_code_keeps_content_terminating_newline() {
    let events = collect("\t|\n\t");
    let text = events
        .iter()
        .find_map(|e| match e {
            Event::Text(t) => Some(t.as_ref()),
            _ => None,
        })
        .expect("indented code block must emit a Text event");
    assert_eq!(
        text, "|\n",
        "pulldown's trailing-blank-line rule for indented code changed; \
         update {MODEL_DOC} §2 and then revisit \
         src/format/mod.rs::normalize_trailing_newline"
    );
}

/// §3a: emphasis delimiters pair within a paragraph.
#[test]
fn emphasis_pairs_within_paragraph() {
    let events = collect("*foo*");
    assert!(
        matches!(events.get(1), Some(Event::Start(Tag::Emphasis))),
        "expected Start(Emphasis) at index 1; got {events:?}. \
         Update {MODEL_DOC} §3 if pulldown changed."
    );
    assert!(matches!(events.get(3), Some(Event::End(TagEnd::Emphasis))));
}

/// §3b: strikethrough is not a pairing boundary — emphasis may
/// straddle a `~~…~~` run on either side.
#[test]
fn emphasis_pairs_across_strikethrough() {
    let events = collect("*~~foo~~*");
    // Expected shape: Para, Emph, Strike, Text(foo), /Strike, /Emph, /Para
    let start_emph = events
        .iter()
        .position(|e| matches!(e, Event::Start(Tag::Emphasis)))
        .expect("must contain Start(Emphasis)");
    let end_emph = events
        .iter()
        .position(|e| matches!(e, Event::End(TagEnd::Emphasis)))
        .expect("must contain End(Emphasis)");
    let start_strike = events
        .iter()
        .position(|e| matches!(e, Event::Start(Tag::Strikethrough)))
        .expect("must contain Start(Strikethrough)");
    let end_strike = events
        .iter()
        .position(|e| matches!(e, Event::End(TagEnd::Strikethrough)))
        .expect("must contain End(Strikethrough)");
    assert!(
        start_emph < start_strike && end_strike < end_emph,
        "expected emphasis to nest *outside* strikethrough; got {events:?}. \
         Update {MODEL_DOC} §3 if pulldown's pairing scope changed."
    );
}

/// §3c: link bodies are a pairing boundary — `*` inside `[…]` cannot
/// pair with a `*` outside.
#[test]
fn link_body_breaks_emphasis_pairing() {
    let events = collect("*[foo*](https://x.com)");
    // Neither asterisk should produce an Emphasis event.
    let has_emphasis = events.iter().any(|e| matches!(e, Event::Start(Tag::Emphasis)));
    assert!(
        !has_emphasis,
        "expected no Emphasis events (link body is a pairing boundary); got {events:?}. \
         Update {MODEL_DOC} §3 if pulldown's link-body pairing rule changed."
    );
}

/// §4: pulldown surfaces the *raw* label bytes in `Tag::Link::id`.
/// Definition-side normalisation lives in mdwright; this test just
/// pins down the convention pulldown uses so the mdwright-side
/// resolver in `src/cm/refs.rs` knows what it's getting.
#[test]
fn reference_label_normalisation_matches() {
    let events = collect("[FOO]: https://x.com\n\n[ foo ][FOO]\n");
    let id = events
        .iter()
        .find_map(|e| match e {
            Event::Start(Tag::Link { id, .. }) => Some(id.as_ref()),
            _ => None,
        })
        .expect("must emit a Reference link");
    assert_eq!(
        id, "FOO",
        "pulldown changed how it surfaces resolved reference IDs; \
         update {MODEL_DOC} §4 and check src/cm/refs.rs"
    );
}

/// §5: type-2 HTML blocks emit one `Html` event per source line.
#[test]
fn html_block_type2_emits_per_line_events() {
    let events = collect("<!-- a\nb\nc -->\nafter\n");
    let html_lines: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::Html(s) => Some(s.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(
        html_lines,
        vec!["<!-- a\n", "b\n", "c -->\n"],
        "pulldown's HTML-block per-line event emission changed; \
         update {MODEL_DOC} §5 before touching src/format/* HTML emit"
    );
}

/// §6: emphasis event ranges span open-delim through close-delim, with
/// `range.end` one past the last delimiter byte.
#[test]
fn emphasis_event_range_spans_delimiters() {
    let ranges: Vec<(String, std::ops::Range<usize>)> = Parser::new_ext("x *foo* y", opts())
        .into_offset_iter()
        .map(|(e, r)| {
            let tag = match e {
                Event::Start(Tag::Emphasis) => "Start(Emphasis)".to_owned(),
                Event::End(TagEnd::Emphasis) => "End(Emphasis)".to_owned(),
                _ => format!("{e:?}"),
            };
            (tag, r)
        })
        .collect();
    let (_, start_range) = ranges
        .iter()
        .find(|(t, _)| t == "Start(Emphasis)")
        .expect("must contain Start(Emphasis)");
    let (_, end_range) = ranges
        .iter()
        .find(|(t, _)| t == "End(Emphasis)")
        .expect("must contain End(Emphasis)");
    assert_eq!(
        (start_range.start, start_range.end),
        (2, 7),
        "Start(Emphasis) range changed; update {MODEL_DOC} §6 and revisit \
         src/format/emit_safety.rs::parses_with_outer_run_at"
    );
    assert_eq!(
        (end_range.start, end_range.end),
        (2, 7),
        "End(Emphasis) range changed; update {MODEL_DOC} §6 and revisit \
         src/format/emit_safety.rs::parses_with_outer_run_at"
    );
}

/// §7: `**foo**` is `Strong`, not `Emphasis(Emphasis)`; `*_foo_*` is
/// two nested `Emphasis` (one per delimiter character).
#[test]
fn strong_distinct_from_nested_emphasis() {
    let strong = collect("**foo**");
    assert!(
        matches!(strong.get(1), Some(Event::Start(Tag::Strong))),
        "expected Strong for **foo**; got {strong:?}. Update {MODEL_DOC} §7."
    );
    let nested = collect("*_foo_*");
    let inner_count = nested
        .iter()
        .filter(|e| matches!(e, Event::Start(Tag::Emphasis)))
        .count();
    assert_eq!(
        inner_count, 2,
        "*_foo_* expected two nested Emphasis runs; got {nested:?}. \
         Update {MODEL_DOC} §7 and revisit \
         src/format/emit_safety.rs::emit_emphasis_safely"
    );
}
