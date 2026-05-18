//! Markdown semantic signatures built from the document parser.
//!
//! This module is the document-owned parser oracle used by formatter
//! verification. It exposes a stable, owned signature of recognised
//! Markdown structure without exposing `pulldown-cmark` events.

use pulldown_cmark::{CowStr, Event, Tag, TagEnd};

use crate::source::{CanonicalSource, Source};
use crate::{ParseError, ParseOptions, parse};

/// A canonical Markdown event stream used for semantic comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownSignature {
    events: Vec<CanonicalEvent>,
}

impl MarkdownSignature {
    /// Return a short description of the first event divergence.
    #[must_use]
    pub fn first_divergence(&self, other: &Self) -> Option<String> {
        if self == other {
            return None;
        }
        for (i, (x, y)) in self.events.iter().zip(other.events.iter()).enumerate() {
            if x != y {
                return Some(format!(
                    "event {i}: source = {:?}; formatted = {:?}",
                    short(x),
                    short(y)
                ));
            }
        }
        let (longer, label) = if self.events.len() > other.events.len() {
            (&self.events, "source")
        } else {
            (&other.events, "formatted")
        };
        let extra = longer
            .get(self.events.len().min(other.events.len()))
            .map_or_else(|| "<eos>".to_owned(), |e| format!("{:?}", short(e)));
        Some(format!(
            "stream length differs ({} vs {}); first extra event on {label}: {extra}",
            self.events.len(),
            other.events.len(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CanonicalEvent {
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartTag {
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
enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EndTag {
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

/// Build a semantic signature under explicit recognition policy.
///
/// # Errors
///
/// Returns [`ParseError`] if parser execution cannot safely recognise
/// the canonicalised source.
pub fn markdown_signature(source: &str, opts: ParseOptions) -> Result<MarkdownSignature, ParseError> {
    let source = Source::new(source);
    let src = CanonicalSource::from_source(&source);
    let mut events: Vec<CanonicalEvent> = Vec::new();
    let mut code_block_depth: u32 = 0;
    let mut pending: Option<String> = None;

    let flush = |pending: &mut Option<String>, events: &mut Vec<CanonicalEvent>| {
        if let Some(buf) = pending.take() {
            let collapsed = collapse_whitespace(&buf);
            if !collapsed.is_empty() {
                events.push(CanonicalEvent::Text(collapsed));
            }
        }
    };

    for ev in parse::collect_events(src, parse::options(opts))? {
        match ev {
            Event::Start(tag) => {
                if matches!(tag, Tag::CodeBlock(_)) {
                    code_block_depth = code_block_depth.saturating_add(1);
                }
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::Start(canonical_start(tag)));
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::CodeBlock) {
                    code_block_depth = code_block_depth.saturating_sub(1);
                }
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::End(canonical_end(tag)));
            }
            Event::Text(s) if code_block_depth > 0 => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::VerbatimText(s.into_string()));
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
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::HardBreak);
            }
            Event::Code(s) => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::Code(s.into_string()));
            }
            Event::InlineMath(s) => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::InlineMath(s.into_string()));
            }
            Event::DisplayMath(s) => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::DisplayMath(s.into_string()));
            }
            Event::Html(s) => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::Html(s.into_string()));
            }
            Event::InlineHtml(s) => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::InlineHtml(s.into_string()));
            }
            Event::FootnoteReference(s) => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::FootnoteReference(s.into_string()));
            }
            Event::Rule => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::Rule);
            }
            Event::TaskListMarker(b) => {
                flush(&mut pending, &mut events);
                events.push(CanonicalEvent::TaskListMarker(b));
            }
        }
    }
    flush(&mut pending, &mut events);
    Ok(MarkdownSignature { events })
}

fn cow_to_string(c: CowStr<'_>) -> String {
    c.into_string()
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

fn short(ev: &CanonicalEvent) -> CanonicalEvent {
    const MAX: usize = 60;
    let clip = |s: &str| {
        if s.chars().count() <= MAX {
            s.to_owned()
        } else {
            let mut t: String = s.chars().take(MAX).collect();
            t.push_str("...");
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
