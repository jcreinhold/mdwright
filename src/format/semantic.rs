//! Semantic-equivalence comparison on pulldown-cmark event streams.
//!
//! The formatter rewrites Markdown source; "did the rewrite change the
//! document's meaning?" is the load-bearing question both the runtime
//! gate (`Document::format_validated`) and the offline correctness
//! suites (property tests, the GFM spec runner, the
//! `fuzz_parse_format` oracle) must answer.
//!
//! We answer it by parsing both sides with pulldown-cmark and
//! comparing their event streams modulo whitespace inside non-verbatim
//! text. Concretely: any run of `Event::Text` / `Event::SoftBreak`
//! within the same inline context is folded into one canonical
//! `Text(collapsed_whitespace)` event. Verbatim regions —
//! `Event::Text` inside a fenced or indented code block,
//! `Event::Code` (inline code), `Event::Html`, `Event::InlineHtml`,
//! `Event::InlineMath`, `Event::DisplayMath` — pass through
//! byte-for-byte; structural events (`Start(Tag)`, `End(TagEnd)`,
//! `HardBreak`, `Rule`, `FootnoteReference`, `TaskListMarker`) are
//! left as-is and compared structurally.
//!
//! Working at the event layer instead of rendered HTML avoids two
//! whole classes of bug we used to chase by post-hoc HTML
//! normalisation: pulldown surfaces the verbatim/non-verbatim
//! distinction structurally (so we never reach for `<pre>` / `<code>`
//! by string scan), and prose rewraps are equivalence-preserving by
//! construction (the soft-break folder yields the same canonical text
//! regardless of where the source put its newlines).
//!
//! Public API:
//!
//! - [`semantically_equivalent`] — boolean predicate used by the
//!   runtime gate.
//! - [`first_divergence`] — diagnostic; returns the index of the
//!   first differing canonical event plus a short human-readable
//!   summary. Used to populate `FormatError::SemanticDivergence
//!   { diff_summary }`.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd};

/// A canonicalised pulldown event used for equality comparison. The
/// only transformations from `Event` are:
///
/// - Runs of `Text` / `SoftBreak` outside verbatim regions are folded
///   into a single `Text` with internal whitespace collapsed.
/// - `Text` inside a fenced or indented code block becomes
///   `VerbatimText` so the normaliser cannot fold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CanonicalEvent {
    Start(StartTag),
    End(EndTag),
    Text(String),
    VerbatimText(String),
    Code(String),
    InlineMath(String),
    DisplayMath(String),
    Html(String),
    InlineHtml(String),
    FootnoteReference(String),
    HardBreak,
    Rule,
    TaskListMarker(bool),
}

/// `Tag<'a>` is borrowed and lifetime-bound to a specific parse. The
/// canonical form owns its strings so two streams from independent
/// `&str`s can compare without lifetime gymnastics. We capture only
/// the fields that matter for equivalence — destinations, titles,
/// languages, alignments — not the original source spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartTag {
    Paragraph,
    Heading(u32),
    BlockQuote,
    CodeBlock { fenced: bool, info: String },
    HtmlBlock,
    List { ordered: bool, start: u64 },
    Item,
    FootnoteDefinition(String),
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Table(Vec<TableAlign>),
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link { dest: String, title: String, id: String },
    Image { dest: String, title: String, id: String },
    MetadataBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EndTag {
    Paragraph,
    Heading(u32),
    BlockQuote,
    CodeBlock,
    HtmlBlock,
    List(bool),
    Item,
    FootnoteDefinition,
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link,
    Image,
    MetadataBlock,
}

/// Parser options shared with the formatter's own walk in
/// `src/ir.rs:234`. Keeping these in lockstep means the gate sees the
/// same event stream the formatter built its IR from.
fn options() -> Options {
    let mut o = Options::empty();
    o.insert(Options::ENABLE_STRIKETHROUGH);
    o.insert(Options::ENABLE_FOOTNOTES);
    o.insert(Options::ENABLE_TABLES);
    o.insert(Options::ENABLE_TASKLISTS);
    o
}

/// Build the canonical event stream for a Markdown source. The
/// returned vector compares equal between two semantically-equivalent
/// sources regardless of where they put their soft line breaks.
#[must_use]
pub(crate) fn canonical_events(source: &str) -> Vec<CanonicalEvent> {
    let mut out: Vec<CanonicalEvent> = Vec::new();
    let mut code_block_depth: u32 = 0;
    let mut pending: Option<String> = None;

    // A buffered Text/SoftBreak run that collapses to the empty
    // string is silently dropped — `Text("")` would otherwise
    // diverge from the formatted-side parse, which never produces an
    // empty Text event for whitespace-only spans (pulldown elides
    // them at the boundary). See the heading-trailing-hash repro
    // (`# ~~a~~ #` → `# ~~a~~`).
    let flush = |pending: &mut Option<String>, out: &mut Vec<CanonicalEvent>| {
        if let Some(buf) = pending.take() {
            let collapsed = collapse_whitespace(&buf);
            if !collapsed.is_empty() {
                out.push(CanonicalEvent::Text(collapsed));
            }
        }
    };

    for ev in Parser::new_ext(source, options()) {
        match ev {
            Event::Start(tag) => {
                if matches!(tag, Tag::CodeBlock(_)) {
                    code_block_depth = code_block_depth.saturating_add(1);
                }
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::Start(canonical_start(tag)));
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::CodeBlock) {
                    code_block_depth = code_block_depth.saturating_sub(1);
                }
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::End(canonical_end(tag)));
            }
            Event::Text(s) if code_block_depth > 0 => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::VerbatimText(into_string_lf(s)));
            }
            Event::Text(s) => {
                pending.get_or_insert_with(String::new).push_str(&s);
            }
            Event::SoftBreak => {
                let buf = pending.get_or_insert_with(String::new);
                if !buf.is_empty() && !buf.ends_with(' ') {
                    buf.push(' ');
                }
            }
            Event::HardBreak => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::HardBreak);
            }
            Event::Code(s) => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::Code(into_string_lf(s)));
            }
            Event::InlineMath(s) => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::InlineMath(into_string_lf(s)));
            }
            Event::DisplayMath(s) => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::DisplayMath(into_string_lf(s)));
            }
            Event::Html(s) => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::Html(into_string_lf(s)));
            }
            Event::InlineHtml(s) => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::InlineHtml(into_string_lf(s)));
            }
            Event::FootnoteReference(s) => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::FootnoteReference(s.into_string()));
            }
            Event::Rule => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::Rule);
            }
            Event::TaskListMarker(b) => {
                flush(&mut pending, &mut out);
                out.push(CanonicalEvent::TaskListMarker(b));
            }
        }
    }
    flush(&mut pending, &mut out);
    out
}

fn cow_to_string(c: CowStr<'_>) -> String {
    c.into_string()
}

/// Convert a `CowStr` to an owned `String` with CRLF / CR collapsed
/// to LF.
///
/// Pulldown normalises line endings for prose text (CM §2.2 — CR,
/// CRLF, LF are equivalent) but preserves the raw bytes inside
/// `Html`, `InlineHtml`, code blocks, and math regions. The
/// formatter, in contrast, runs every output through
/// `normalize_line_endings_lf`, so a source `<?\r` (Html with CR)
/// emits as `<?\n` (Html with LF). Without this collapse the
/// canonical event comparator treats the two byte streams as
/// distinct, generating a spurious semantic-divergence report even
/// though CM considers them equivalent.
fn into_string_lf(s: CowStr<'_>) -> String {
    let mut s = s.into_string();
    super::normalize_line_endings_lf(&mut s);
    s
}

#[allow(clippy::too_many_lines, reason = "one-to-one variant mapping")]
fn canonical_start(tag: Tag<'_>) -> StartTag {
    use pulldown_cmark::{Alignment, CodeBlockKind, HeadingLevel};
    match tag {
        Tag::Paragraph => StartTag::Paragraph,
        Tag::Heading { level, .. } => StartTag::Heading(match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }),
        Tag::BlockQuote(_) => StartTag::BlockQuote,
        Tag::CodeBlock(kind) => match kind {
            CodeBlockKind::Fenced(info) => StartTag::CodeBlock {
                fenced: true,
                info: info.into_string(),
            },
            CodeBlockKind::Indented => StartTag::CodeBlock {
                fenced: false,
                info: String::new(),
            },
        },
        Tag::HtmlBlock => StartTag::HtmlBlock,
        Tag::List(start) => StartTag::List {
            ordered: start.is_some(),
            start: start.unwrap_or(0),
        },
        Tag::Item => StartTag::Item,
        Tag::FootnoteDefinition(label) => StartTag::FootnoteDefinition(label.into_string()),
        Tag::DefinitionList => StartTag::DefinitionList,
        Tag::DefinitionListTitle => StartTag::DefinitionListTitle,
        Tag::DefinitionListDefinition => StartTag::DefinitionListDefinition,
        Tag::Table(alignments) => StartTag::Table(
            alignments
                .into_iter()
                .map(|a| match a {
                    Alignment::None => TableAlign::None,
                    Alignment::Left => TableAlign::Left,
                    Alignment::Center => TableAlign::Center,
                    Alignment::Right => TableAlign::Right,
                })
                .collect(),
        ),
        Tag::TableHead => StartTag::TableHead,
        Tag::TableRow => StartTag::TableRow,
        Tag::TableCell => StartTag::TableCell,
        Tag::Emphasis => StartTag::Emphasis,
        Tag::Strong => StartTag::Strong,
        Tag::Strikethrough => StartTag::Strikethrough,
        Tag::Superscript => StartTag::Superscript,
        Tag::Subscript => StartTag::Subscript,
        Tag::Link {
            dest_url, title, id, ..
        } => StartTag::Link {
            dest: cow_to_string(dest_url),
            title: cow_to_string(title),
            id: cow_to_string(id),
        },
        Tag::Image {
            dest_url, title, id, ..
        } => StartTag::Image {
            dest: cow_to_string(dest_url),
            title: cow_to_string(title),
            id: cow_to_string(id),
        },
        Tag::MetadataBlock(_) => StartTag::MetadataBlock,
    }
}

fn canonical_end(tag: TagEnd) -> EndTag {
    use pulldown_cmark::HeadingLevel;
    match tag {
        TagEnd::Paragraph => EndTag::Paragraph,
        TagEnd::Heading(level) => EndTag::Heading(match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }),
        TagEnd::BlockQuote(_) => EndTag::BlockQuote,
        TagEnd::CodeBlock => EndTag::CodeBlock,
        TagEnd::HtmlBlock => EndTag::HtmlBlock,
        TagEnd::List(ordered) => EndTag::List(ordered),
        TagEnd::Item => EndTag::Item,
        TagEnd::FootnoteDefinition => EndTag::FootnoteDefinition,
        TagEnd::DefinitionList => EndTag::DefinitionList,
        TagEnd::DefinitionListTitle => EndTag::DefinitionListTitle,
        TagEnd::DefinitionListDefinition => EndTag::DefinitionListDefinition,
        TagEnd::Table => EndTag::Table,
        TagEnd::TableHead => EndTag::TableHead,
        TagEnd::TableRow => EndTag::TableRow,
        TagEnd::TableCell => EndTag::TableCell,
        TagEnd::Emphasis => EndTag::Emphasis,
        TagEnd::Strong => EndTag::Strong,
        TagEnd::Strikethrough => EndTag::Strikethrough,
        TagEnd::Superscript => EndTag::Superscript,
        TagEnd::Subscript => EndTag::Subscript,
        TagEnd::Link => EndTag::Link,
        TagEnd::Image => EndTag::Image,
        TagEnd::MetadataBlock(_) => EndTag::MetadataBlock,
    }
}

/// Collapse internal whitespace runs to single spaces and trim
/// leading/trailing whitespace. Matches the browser-rendering rule
/// for non-preformatted text: any run of `[ \t\r\n\f\v]` is one
/// space; edges are stripped.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            in_ws = true;
        } else {
            if in_ws && !out.is_empty() {
                out.push(' ');
            }
            in_ws = false;
            out.push(c);
        }
    }
    out
}

/// True iff the two Markdown sources parse to the same canonical
/// event stream. The intended invariant for `Document::format`: the
/// formatted output is semantically equivalent to its source.
///
/// The runtime gate ([`crate::Document::format_validated`]), the
/// property tests, the GFM-spec runner, and the `fuzz_parse_format`
/// oracle all route through this single definition.
#[must_use]
pub fn semantically_equivalent(source: &str, formatted: &str) -> bool {
    canonical_events(source) == canonical_events(formatted)
}

/// If `source` and `formatted` are not semantically equivalent,
/// return a short human-readable description of the first divergent
/// event pair. Returns `None` if the streams agree.
///
/// Used to populate `FormatError::SemanticDivergence::diff_summary`
/// so the failure message points at the actual disagreement instead
/// of dumping two HTML strings.
#[must_use]
pub(crate) fn first_divergence(source: &str, formatted: &str) -> Option<String> {
    let a = canonical_events(source);
    let b = canonical_events(formatted);
    if a == b {
        return None;
    }
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            return Some(format!(
                "event {i}: source = {:?}; formatted = {:?}",
                short(x),
                short(y)
            ));
        }
    }
    let (longer, label) = if a.len() > b.len() {
        (&a, "source")
    } else {
        (&b, "formatted")
    };
    let extra = longer
        .get(a.len().min(b.len()))
        .map_or_else(|| "<eos>".to_owned(), |e| format!("{:?}", short(e)));
    Some(format!(
        "stream length differs ({} vs {}); first extra event on {label}: {extra}",
        a.len(),
        b.len(),
    ))
}

/// Truncate text payloads in the debug rendering so error messages
/// stay readable.
fn short(ev: &CanonicalEvent) -> CanonicalEvent {
    const MAX: usize = 60;
    let clip = |s: &str| {
        if s.chars().count() <= MAX {
            s.to_owned()
        } else {
            let mut t: String = s.chars().take(MAX).collect();
            t.push('…');
            t
        }
    };
    match ev {
        CanonicalEvent::Text(s) => CanonicalEvent::Text(clip(s)),
        CanonicalEvent::VerbatimText(s) => CanonicalEvent::VerbatimText(clip(s)),
        CanonicalEvent::Code(s) => CanonicalEvent::Code(clip(s)),
        CanonicalEvent::Html(s) => CanonicalEvent::Html(clip(s)),
        CanonicalEvent::InlineHtml(s) => CanonicalEvent::InlineHtml(clip(s)),
        other @ (CanonicalEvent::Start(_)
        | CanonicalEvent::End(_)
        | CanonicalEvent::InlineMath(_)
        | CanonicalEvent::DisplayMath(_)
        | CanonicalEvent::FootnoteReference(_)
        | CanonicalEvent::HardBreak
        | CanonicalEvent::Rule
        | CanonicalEvent::TaskListMarker(_)) => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::semantically_equivalent;

    #[test]
    fn prose_rewrap_inside_paragraph_is_equivalent() {
        let a = "alpha beta gamma\ndelta epsilon zeta\n";
        let b = "alpha beta gamma delta\nepsilon zeta\n";
        assert!(semantically_equivalent(a, b));
    }

    #[test]
    fn prose_rewrap_inside_blockquote_is_equivalent() {
        let a = "> alpha beta gamma\n> delta epsilon zeta\n";
        let b = "> alpha beta gamma delta epsilon\n> zeta\n";
        assert!(semantically_equivalent(a, b));
    }

    #[test]
    fn whitespace_change_inside_fenced_code_is_rejected() {
        let a = "```\nfoo\nbar\n```\n";
        let b = "```\nfoo bar\n```\n";
        assert!(!semantically_equivalent(a, b));
    }

    #[test]
    fn whitespace_change_inside_inline_code_is_rejected() {
        let a = "see `x  y` here\n";
        let b = "see `x y` here\n";
        assert!(!semantically_equivalent(a, b));
    }

    #[test]
    fn dropped_emphasis_is_rejected() {
        let a = "foo *bar* baz\n";
        let b = "foo bar baz\n";
        assert!(!semantically_equivalent(a, b));
    }

    #[test]
    fn link_target_change_is_rejected() {
        let a = "[label](https://a.example)\n";
        let b = "[label](https://b.example)\n";
        assert!(!semantically_equivalent(a, b));
    }

    #[test]
    fn link_text_rewrap_is_equivalent() {
        let a = "[label one\ntwo](https://x.example)\n";
        let b = "[label one two](https://x.example)\n";
        assert!(semantically_equivalent(a, b));
    }

    #[test]
    fn table_cell_whitespace_rewrap_is_equivalent() {
        let a = "| a | b |\n|---|---|\n| x | y |\n";
        let b = "| a   | b   |\n| --- | --- |\n| x   | y   |\n";
        assert!(semantically_equivalent(a, b));
    }

    #[test]
    fn dropped_heading_level_is_rejected() {
        let a = "## foo\n";
        let b = "### foo\n";
        assert!(!semantically_equivalent(a, b));
    }

    #[test]
    fn hard_break_distinct_from_soft_break() {
        let a = "foo  \nbar\n";
        let b = "foo\nbar\n";
        assert!(!semantically_equivalent(a, b));
    }

    #[test]
    fn fenced_info_string_change_is_rejected() {
        let a = "```rust\nfoo\n```\n";
        let b = "```python\nfoo\n```\n";
        assert!(!semantically_equivalent(a, b));
    }

    #[test]
    fn identical_inputs_equivalent() {
        let a = "## heading\n\nparagraph one.\n\n- item a\n- item b\n";
        assert!(semantically_equivalent(a, a));
    }
}
