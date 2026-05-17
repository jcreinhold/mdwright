//! Parsed-document intermediate representation.
//!
//! The IR is a curated, opinionated view of a Markdown document, built
//! once at parse time and consumed by lint rules through the public
//! [`Document`](crate::Document) façade. It hides two things from rule
//! authors:
//!
//! - The pulldown-cmark event stream and its peculiarities (Text-event
//!   byte ranges that omit preceding `\\` escapes; tight-list items
//!   that bypass the `Paragraph` tag; container ranges that retain
//!   blockquote markers on inner lines).
//! - The post-parse work needed to recover information the parser
//!   doesn't surface directly: link-reference definitions, code-block
//!   info strings, list-marker bytes.
//!
//! The data-carrier types ([`TextSlice`], [`InlineCode`], [`Heading`],
//! [`ListGroup`], etc.) are also the public types returned by
//! `Document`'s accessors. Their fields are public — they're value
//! objects, not abstractions, and information-hiding on a position
//! record buys nothing.

use std::ops::Range;
use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use regex::Regex;

use crate::cm::refs::{ReferenceTable, build_reference_table};
use crate::line_index::LineIndex;
use crate::parse;
use crate::source::CanonicalSource;
use crate::tree::{Tree, TreeBuilder};
use crate::util::regex::compile_static;

/// A borrowed slice of source bytes plus its absolute byte range.
/// The minimal record every rule needs to emit a diagnostic.
#[derive(Clone, Debug)]
pub struct TextSlice {
    pub text: String,
    pub byte_offset: usize,
    pub raw_range: Range<usize>,
}

/// One inline code span. `text` excludes the surrounding backticks;
/// `raw_range` covers them.
#[derive(Clone, Debug)]
pub struct InlineCode {
    pub text: String,
    pub byte_offset: usize,
    pub raw_range: Range<usize>,
}

/// One fenced or indented code block.
///
/// `text` is the body excluding fence lines; `raw_range` covers the
/// whole block including fences. `info` is the fence info string
/// (the language tag); empty for indented blocks.
#[derive(Clone, Debug)]
pub struct CodeBlock {
    pub text: String,
    pub byte_offset: usize,
    pub raw_range: Range<usize>,
    pub info: String,
    pub fenced: bool,
}

/// One HTML block (`CommonMark` §4.6).
#[derive(Clone, Debug)]
pub struct HtmlBlock {
    pub text: String,
    pub byte_offset: usize,
    pub raw_range: Range<usize>,
}

/// One inline HTML tag (open, close, self-closing, comment, etc.)
/// embedded in a paragraph.
#[derive(Clone, Debug)]
pub struct InlineHtml {
    pub text: String,
    pub byte_offset: usize,
    pub raw_range: Range<usize>,
}

/// One ATX or setext heading. `text` is the trimmed text content
/// (`#` markers and trailing whitespace stripped); `raw_range` covers
/// the whole heading line(s).
#[derive(Clone, Debug)]
pub struct Heading {
    pub text: String,
    pub byte_offset: usize,
    pub raw_range: Range<usize>,
    /// 1 through 6 for `H1`..`H6`.
    pub level: u32,
}

/// A contiguous list at one indentation depth. Nested lists are
/// distinct `ListGroup` entries.
#[derive(Clone, Debug)]
pub struct ListGroup {
    pub raw_range: Range<usize>,
    pub ordered: bool,
    pub items: Vec<ListItem>,
}

/// One item within a [`ListGroup`].
#[derive(Clone, Debug)]
pub struct ListItem {
    pub raw_range: Range<usize>,
    /// Byte at the start of the marker (`-`, `*`, `+`, or `'0'..='9'`).
    /// For ordered lists this is the first digit of the index.
    pub marker_byte: u8,
}

/// Frontmatter at the document head. Carries the raw slice plus a
/// tag for which delimiter the source used so the formatter can emit
/// the same opening and closing markers.
#[derive(Clone, Debug)]
pub struct Frontmatter {
    pub slice: TextSlice,
    pub delimiter: FrontmatterDelimiter,
}

/// Frontmatter fence style. `Yaml` uses `---` open and `---`/`...`
/// close; `Toml` uses `+++` for both.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrontmatterDelimiter {
    Yaml,
    Toml,
}

/// One link reference definition (`[label]: dest`).
///
/// The lint-rule surface — produced by [`crate::Document::link_defs`]
/// from the document's [`ReferenceTable`](crate::cm::refs::ReferenceTable).
/// Pulldown-cmark 0.13 does not emit definition events, so the
/// authoritative scan lives in [`crate::cm::refs::build_reference_table`].
#[derive(Clone, Debug)]
pub struct LinkDef<'a> {
    pub label: &'a str,
    pub dest: &'a str,
    /// Optional title from `"…"`, `'…'`, or `(…)` after the
    /// destination. Borrowed from the [`ReferenceTable`]; surrounding
    /// quotes / parens are excluded.
    pub title: Option<&'a str>,
    pub raw_range: Range<usize>,
}

/// One inline suppression directive parsed from a Markdown HTML
/// comment.
///
/// A single filter in [`Document::lint`](crate::Document::lint) uses
/// these to drop diagnostics — no rule code knows that suppressions
/// exist. The comment must live on its own source line with up to
/// three spaces of leading indentation.
///
/// Recognised forms:
///
/// - `<!-- mdwright: allow rule-a[, rule-b] -->` — silences the
///   listed rules on the *next block*.
/// - `<!-- mdwright: allow-next-line rule-a[, rule-b] -->` —
///   silences on the immediately following source line.
/// - `<!-- mdwright: disable [rule-a, ...] -->` — opens a region
///   ending at the matching `enable` (or end of file). An empty
///   rule list means every known rule.
/// - `<!-- mdwright: enable [rule-a, ...] -->` — closes a region.
/// - `<!-- mdwright: disable-all -->` / `<!-- mdwright: enable-all -->`
///   — convenience aliases for `disable` / `enable` with no names.
#[derive(Clone, Debug)]
pub struct Suppression {
    pub kind: SuppressionKind,
    /// Rule names parsed from the comment body. Empty for the bare
    /// `disable` / `enable` forms and for `disable-all` / `enable-all`;
    /// the suppression map expands empty to "every known rule".
    pub rules: Vec<String>,
    pub raw_range: Range<usize>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SuppressionKind {
    Allow { scope: AllowScope },
    Disable,
    Enable,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AllowScope {
    /// The next block (paragraph, heading, code block, list group).
    Block,
    /// The single source line immediately after the comment.
    NextLine,
}

/// The parsed document. Owned by [`Document`](crate::Document); fields
/// are `pub(crate)` so the façade can hand out borrowed views.
#[derive(Debug)]
pub(crate) struct Ir {
    pub(crate) prose_chunks: Vec<TextSlice>,
    pub(crate) inline_codes: Vec<InlineCode>,
    pub(crate) code_blocks: Vec<CodeBlock>,
    pub(crate) html_blocks: Vec<HtmlBlock>,
    pub(crate) inline_html: Vec<InlineHtml>,
    pub(crate) headings: Vec<Heading>,
    pub(crate) list_groups: Vec<ListGroup>,
    pub(crate) refs: ReferenceTable,
    pub(crate) suppressions: Vec<Suppression>,
    pub(crate) frontmatter: Option<Frontmatter>,
    pub(crate) admonitions: Vec<AdmonitionRegion>,
    pub(crate) math_regions: Vec<MathRegion>,
    pub(crate) math_errors: Vec<MathError>,
    pub(crate) line_index: LineIndex,
    pub(crate) tree: Tree,
}

/// One mkdocs-style admonition region in source order.
///
/// Detected by a post-parse line scan (see [`scan_admonitions`]).
/// The formatter emits the region's `text` byte-verbatim and skips
/// the tree nodes whose `raw_range` falls inside `range`.
#[derive(Clone, Debug)]
pub(crate) struct AdmonitionRegion {
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
}

use crate::cm::math::MathRegion;
use crate::cm::math::scan::{MathConfig, scan_math_regions};
use crate::cm::math::span::MathError;

impl Ir {
    #[tracing::instrument(level = "info", name = "Ir::parse", skip(src), fields(len = src.as_str().len()))]
    pub(crate) fn parse(src: CanonicalSource<'_>) -> Self {
        let source = src.as_str();
        let line_index = LineIndex::new(source);
        let (fm_end, frontmatter) = split_frontmatter(source);
        let body = src.trusted_subrange(fm_end..source.len());

        let mut builder = Builder {
            source,
            in_code_block: 0,
            heading_stack: Vec::new(),
            list_stack: Vec::new(),
            code_block_stack: Vec::new(),
            blockquote_stack: Vec::new(),
            blockquote_ranges: Vec::new(),
            list_item_ranges: Vec::new(),
            prose_chunks: Vec::new(),
            inline_codes: Vec::new(),
            code_blocks: Vec::new(),
            html_blocks: Vec::new(),
            inline_html: Vec::new(),
            headings: Vec::new(),
            list_groups: Vec::new(),
        };
        // Collect pulldown events once with absolute byte ranges. The
        // reference table is built from this event stream (pulldown's
        // own §4.7 resolution is authoritative); the flat IR is built
        // first (the math scanner depends on the exclusion zones it
        // collects), then math regions are computed, then the tree
        // is built — the tree builder needs math regions so it can
        // splice `NodeKind::Math` leaves at recognised positions.
        let events: Vec<(Event<'_>, Range<usize>)> = parse::events_with_offsets(body, parse::FORMATTER_OPTIONS)
            .map(|(e, r)| {
                let abs = r.start.saturating_add(fm_end)..r.end.saturating_add(fm_end);
                (e, abs)
            })
            .collect();
        for (event, abs) in &events {
            builder.handle(event.clone(), abs.clone());
        }
        tracing::debug!(events = events.len(), "flat-IR walk complete");

        // Math regions: the scanner excludes code spans / blocks /
        // HTML blocks / inline HTML (regions where `\[` / `\(` / `$`
        // are not math). Transparent runs (blockquote `>` markers
        // and list-item continuation indents) let the recogniser
        // scan across container prefixes without those bytes leaking
        // into the math body.
        let transparent_runs = compute_transparent_runs(source, &builder.blockquote_ranges, &builder.list_item_ranges);
        let (math_regions, math_errors) = scan_math_regions(
            source,
            &builder.inline_codes,
            &builder.code_blocks,
            &builder.html_blocks,
            &builder.inline_html,
            &transparent_runs,
            MathConfig::default(),
        );

        let mut tree_builder = TreeBuilder::new(source, &math_regions);
        for (event, abs) in &events {
            tree_builder.handle(event, abs.clone());
        }
        tracing::debug!(nodes = tree_builder.arena_len(), "tree walk complete");

        let bare_events: Vec<Event<'_>> = events.into_iter().map(|(e, _)| e).collect();
        let refs = build_reference_table(&bare_events, source);
        let suppressions = scan_suppressions(&builder.html_blocks);
        let admonitions = scan_admonitions(source, &builder.code_blocks);
        let tree = tree_builder.finalize(&refs);

        Self {
            prose_chunks: builder.prose_chunks,
            inline_codes: builder.inline_codes,
            code_blocks: builder.code_blocks,
            html_blocks: builder.html_blocks,
            inline_html: builder.inline_html,
            headings: builder.headings,
            list_groups: builder.list_groups,
            refs,
            suppressions,
            frontmatter,
            admonitions,
            math_regions,
            math_errors,
            line_index,
            tree,
        }
    }

    pub(crate) fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    /// Test-only convenience that builds a [`Source`] from `src` and
    /// then parses through the chokepoint. Production code constructs
    /// a [`CanonicalSource`] once at [`crate::Document::parse`] and
    /// passes it down.
    ///
    /// [`Source`]: crate::source::Source
    /// [`CanonicalSource`]: crate::source::CanonicalSource
    #[cfg(test)]
    pub(crate) fn parse_str(src: &str) -> Self {
        let source = crate::source::Source::new(src);
        Self::parse(CanonicalSource::from_source(&source))
    }
}

/// Walks the pulldown-cmark event stream and accumulates IR fields.
/// One pass per document; no borrow of the IR's final shape.
struct Builder<'a> {
    source: &'a str,
    in_code_block: u32,
    /// Stack of open headings: `(start_byte, level)`.
    heading_stack: Vec<(usize, u32)>,
    /// Stack of open lists; each entry holds the list's start offset,
    /// whether it is ordered, and items collected so far.
    list_stack: Vec<OpenList>,
    /// Stack of open code blocks: `(start_byte, info, fenced)`.
    code_block_stack: Vec<(usize, String, bool)>,
    /// Stack of open blockquotes: `start_byte`. Closed entries are
    /// drained into [`Self::blockquote_ranges`] for the
    /// transparent-runs computation.
    blockquote_stack: Vec<usize>,
    /// Closed blockquote ranges, in close order. Used by
    /// [`compute_transparent_runs`] to identify lines whose leading
    /// `>` marker the math recogniser must treat as non-content.
    blockquote_ranges: Vec<Range<usize>>,
    /// Closed list-item ranges paired with their continuation-indent
    /// width (from [`item_indent`]). Used by
    /// [`compute_transparent_runs`] for continuation-line indentation.
    list_item_ranges: Vec<(Range<usize>, u8)>,
    prose_chunks: Vec<TextSlice>,
    inline_codes: Vec<InlineCode>,
    code_blocks: Vec<CodeBlock>,
    html_blocks: Vec<HtmlBlock>,
    inline_html: Vec<InlineHtml>,
    headings: Vec<Heading>,
    list_groups: Vec<ListGroup>,
}

struct OpenList {
    start: usize,
    ordered: bool,
    items: Vec<ListItem>,
}

impl Builder<'_> {
    #[allow(clippy::wildcard_enum_match_arm)] // many irrelevant Event variants
    fn handle(&mut self, event: Event<'_>, range: Range<usize>) {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag, range),
            Event::Text(_) => self.push_prose(range),
            Event::Code(_) => self.push_inline_code(range),
            Event::Html(_) => self.push_html_block(range),
            Event::InlineHtml(_) => self.push_inline_html(range),
            // SoftBreak, HardBreak, Rule, FootnoteReference,
            // TaskListMarker, InlineMath, DisplayMath — none carry
            // bytes we lint as their own chunks. Math events are
            // disabled in Options; if they appear, ignore them.
            _ => {}
        }
    }

    #[allow(clippy::wildcard_enum_match_arm)] // many irrelevant Tag variants
    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.heading_stack.push((range.start, level as u32));
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = self.in_code_block.saturating_add(1);
                let (info, fenced) = match kind {
                    CodeBlockKind::Fenced(s) => (s.into_string(), true),
                    CodeBlockKind::Indented => (String::new(), false),
                };
                self.code_block_stack.push((range.start, info, fenced));
            }
            Tag::List(start) => {
                self.list_stack.push(OpenList {
                    start: range.start,
                    ordered: start.is_some(),
                    items: Vec::new(),
                });
            }
            Tag::Item => {
                // Use the parent list's `ordered` flag to scan for the
                // right marker class; see tree::derive_list_marker_byte
                // for why `first_non_whitespace_byte(range.start)` is
                // unsafe across container nesting.
                let ordered = self.list_stack.last().is_some_and(|l| l.ordered);
                let marker_byte = derive_item_marker_byte(self.source, range.clone(), ordered).unwrap_or(b'-');
                let indent = item_continuation_width(self.source, &range);
                self.list_item_ranges.push((range.clone(), indent));
                if let Some(open) = self.list_stack.last_mut() {
                    open.items.push(ListItem {
                        raw_range: range,
                        marker_byte,
                    });
                }
            }
            Tag::BlockQuote(_) => {
                self.blockquote_stack.push(range.start);
            }
            #[allow(clippy::wildcard_enum_match_arm)]
            _ => {}
        }
    }

    #[allow(clippy::wildcard_enum_match_arm)] // many irrelevant TagEnd variants
    fn end(&mut self, tag: TagEnd, range: Range<usize>) {
        match tag {
            TagEnd::Heading(_) => {
                if let Some((start, level)) = self.heading_stack.pop() {
                    let end = range.end;
                    let raw = self.source.get(start..end).unwrap_or("");
                    let (trimmed, off) = trim_heading(raw);
                    self.headings.push(Heading {
                        text: trimmed.to_owned(),
                        byte_offset: start.saturating_add(off),
                        raw_range: start..end,
                        level,
                    });
                }
            }
            TagEnd::CodeBlock => {
                self.in_code_block = self.in_code_block.saturating_sub(1);
                if let Some((start, info, fenced)) = self.code_block_stack.pop() {
                    let end = range.end;
                    let raw = self.source.get(start..end).unwrap_or("");
                    self.code_blocks.push(CodeBlock {
                        text: raw.to_owned(),
                        byte_offset: start,
                        raw_range: start..end,
                        info,
                        fenced,
                    });
                }
            }
            TagEnd::List(_) => {
                if let Some(open) = self.list_stack.pop() {
                    self.list_groups.push(ListGroup {
                        raw_range: open.start..range.end,
                        ordered: open.ordered,
                        items: open.items,
                    });
                }
            }
            TagEnd::BlockQuote(_) => {
                if let Some(start) = self.blockquote_stack.pop() {
                    self.blockquote_ranges.push(start..range.end);
                }
            }
            #[allow(clippy::wildcard_enum_match_arm)]
            _ => {}
        }
    }

    fn push_prose(&mut self, range: Range<usize>) {
        if self.in_code_block > 0 {
            return;
        }
        // Recover a leading backslash that pulldown-cmark consumed as
        // an escape. The escape is always exactly one byte (`\`) and
        // sits immediately before the Text event's range.
        let bytes = self.source.as_bytes();
        let start = if range.start > 0 && bytes.get(range.start.saturating_sub(1)) == Some(&b'\\') {
            range.start.saturating_sub(1)
        } else {
            range.start
        };
        let end = range.end;
        let Some(text) = self.source.get(start..end) else {
            return;
        };
        self.prose_chunks.push(TextSlice {
            text: text.to_owned(),
            byte_offset: start,
            raw_range: start..end,
        });
    }

    fn push_inline_code(&mut self, range: Range<usize>) {
        let raw = self.source.get(range.clone()).unwrap_or("");
        let lead = raw.bytes().take_while(|&b| b == b'`').count();
        let trail = raw.bytes().rev().take_while(|&b| b == b'`').count();
        let (content_start, content_end) = if lead == 0 || trail == 0 || lead.saturating_add(trail) >= raw.len() {
            (range.start, range.end)
        } else {
            (range.start.saturating_add(lead), range.end.saturating_sub(trail))
        };
        let Some(text) = self.source.get(content_start..content_end) else {
            return;
        };
        self.inline_codes.push(InlineCode {
            text: text.to_owned(),
            byte_offset: content_start,
            raw_range: range,
        });
    }

    fn push_html_block(&mut self, range: Range<usize>) {
        let Some(text) = self.source.get(range.clone()) else {
            return;
        };
        self.html_blocks.push(HtmlBlock {
            text: text.to_owned(),
            byte_offset: range.start,
            raw_range: range,
        });
    }

    fn push_inline_html(&mut self, range: Range<usize>) {
        let Some(text) = self.source.get(range.clone()) else {
            return;
        };
        self.inline_html.push(InlineHtml {
            text: text.to_owned(),
            byte_offset: range.start,
            raw_range: range,
        });
    }
}

/// First non-whitespace byte at or after `start`. Used to recover a
/// list item's marker character, which may be indented under nested
/// lists.
/// Scan the source range for the first byte matching the legal list
/// marker class. Mirrors `tree::derive_list_marker_byte`; pulldown's
/// item range can include parent-container marker bytes when the
/// separator after the parent's marker is a tab (see
/// `fuzz_blockquote_tab_list_marker.in`), so the naive "first
/// non-whitespace byte at range.start" scan returns the parent's
/// marker, not the item's.
fn derive_item_marker_byte(source: &str, range: core::ops::Range<usize>, ordered: bool) -> Option<u8> {
    source.as_bytes().get(range)?.iter().copied().find(|b| {
        if ordered {
            b.is_ascii_digit()
        } else {
            matches!(b, b'-' | b'*' | b'+')
        }
    })
}

/// Byte count from the start of the item's first non-blank line up
/// to and including the single space after the marker. Drives the
/// list-item branch of [`compute_transparent_runs`]: continuation
/// lines of the item have this many leading bytes available to peel.
///
/// Counts the marker's own leading indentation (so a nested item
/// whose marker sits at column 2 reports a width that includes those
/// two spaces). This makes the result usable directly as a "strip
/// this many bytes" instruction on continuation lines, even when
/// the item is nested under another list or blockquote.
fn item_continuation_width(source: &str, raw_range: &Range<usize>) -> u8 {
    let bytes = source.as_bytes().get(raw_range.clone()).unwrap_or(&[]);
    let mut i = 0usize;
    loop {
        let line_start = i;
        while bytes.get(i).is_some_and(|&b| b != b'\n') {
            i = i.saturating_add(1);
        }
        let line = bytes.get(line_start..i).unwrap_or(&[]);
        if line.iter().any(|b| !matches!(*b, b' ' | b'\t' | b'\r')) {
            let mut j = 0usize;
            while line.get(j).is_some_and(|b| matches!(*b, b' ' | b'\t')) {
                j = j.saturating_add(1);
            }
            if line.get(j).is_some_and(u8::is_ascii_digit) {
                while line.get(j).is_some_and(u8::is_ascii_digit) {
                    j = j.saturating_add(1);
                }
                if matches!(line.get(j), Some(b'.' | b')')) {
                    j = j.saturating_add(1);
                } else {
                    return 0;
                }
            } else if matches!(line.get(j), Some(b'-' | b'*' | b'+')) {
                j = j.saturating_add(1);
            } else {
                return 0;
            }
            if line.get(j) == Some(&b' ') {
                j = j.saturating_add(1);
            }
            return u8::try_from(j).unwrap_or(u8::MAX);
        }
        if i >= bytes.len() {
            return 0;
        }
        i = i.saturating_add(1);
    }
}

/// Identify byte ranges the math recogniser must treat as if they
/// don't exist: blockquote `>` markers (plus the optional following
/// space) and list-item continuation indentation on continuation
/// lines.
///
/// One run per line at most. Sorted by start, non-overlapping.
/// Top-level prose (no container context) returns an empty `Vec`,
/// keeping the recogniser's hot path allocation-free.
fn compute_transparent_runs(
    source: &str,
    blockquote_ranges: &[Range<usize>],
    list_item_ranges: &[(Range<usize>, u8)],
) -> Vec<Range<usize>> {
    if blockquote_ranges.is_empty() && list_item_ranges.is_empty() {
        return Vec::new();
    }
    let bytes = source.as_bytes();
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut line_start = 0usize;
    while line_start <= bytes.len() {
        let line_end = bytes
            .get(line_start..)
            .and_then(|s| s.iter().position(|&b| b == b'\n'))
            .map_or(bytes.len(), |n| line_start.saturating_add(n));
        let mut cursor = line_start;
        loop {
            // Blockquote peel: ≤3 leading spaces, then `>`, then one
            // optional space. Requires that some blockquote_range
            // covers the cursor.
            let mut spaces = 0usize;
            while spaces < 3 && bytes.get(cursor.saturating_add(spaces)).copied() == Some(b' ') {
                spaces = spaces.saturating_add(1);
            }
            let marker_pos = cursor.saturating_add(spaces);
            if marker_pos < line_end
                && bytes.get(marker_pos).copied() == Some(b'>')
                && blockquote_ranges.iter().any(|r| r.start <= cursor && cursor < r.end)
            {
                cursor = marker_pos.saturating_add(1);
                if cursor < line_end && bytes.get(cursor).copied() == Some(b' ') {
                    cursor = cursor.saturating_add(1);
                }
                continue;
            }
            // List-item continuation peel: pick the deepest item
            // whose first line lies strictly before this line and
            // which still covers the cursor.
            let item_width = list_item_ranges
                .iter()
                .filter(|(r, _)| r.start < line_start && cursor < r.end)
                .map(|(r, w)| (r.start, usize::from(*w)))
                .max_by_key(|(s, _)| *s)
                .map(|(_, w)| w);
            if let Some(width) = item_width {
                let mut consumed = 0usize;
                while consumed < width
                    && cursor.saturating_add(consumed) < line_end
                    && bytes.get(cursor.saturating_add(consumed)).copied() == Some(b' ')
                {
                    consumed = consumed.saturating_add(1);
                }
                if consumed > 0 {
                    cursor = cursor.saturating_add(consumed);
                    continue;
                }
            }
            break;
        }
        if cursor > line_start {
            out.push(line_start..cursor);
        }
        if line_end >= bytes.len() {
            break;
        }
        line_start = line_end.saturating_add(1);
    }
    out
}

/// Strip ATX `#` markers and surrounding whitespace from a heading's
/// raw source range. Returns the trimmed text plus the byte offset of
/// the first text byte relative to the range start. Handles ATX
/// (`## Foo`) and setext (`Foo\n---`) shapes — for setext, take the
/// text up to the first newline.
fn trim_heading(raw: &str) -> (&str, usize) {
    let body = raw.strip_suffix('\n').unwrap_or(raw);
    let body = body.split_once('\n').map_or(body, |(first, _)| first);
    let lead_hashes = body.bytes().take_while(|&b| b == b'#').count();
    let after_hashes = body.get(lead_hashes..).unwrap_or("");
    let lead_ws = after_hashes.bytes().take_while(|&b| b == b' ' || b == b'\t').count();
    let inner_start = lead_hashes.saturating_add(lead_ws);
    let inner = body.get(inner_start..).unwrap_or("");
    let trail_ws = inner.bytes().rev().take_while(|&b| b == b' ' || b == b'\t').count();
    let after_trail_ws = inner.len().saturating_sub(trail_ws);
    let no_trail_ws = inner.get(..after_trail_ws).unwrap_or("");
    let trail_hashes = no_trail_ws.bytes().rev().take_while(|&b| b == b'#').count();
    let after_trail_hashes = no_trail_ws.len().saturating_sub(trail_hashes);
    let no_trail_hashes = no_trail_ws.get(..after_trail_hashes).unwrap_or("");
    let final_trail = no_trail_hashes
        .bytes()
        .rev()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count();
    let final_end = no_trail_hashes.len().saturating_sub(final_trail);
    let text = no_trail_hashes.get(..final_end).unwrap_or("");
    (text, inner_start)
}

/// Detect and split off frontmatter at the document start. Returns
/// the byte offset where the body begins and an optional
/// [`Frontmatter`] covering the region.
///
/// Accepts two delimiters:
///
/// - `---\n…\n---\n` (or `…\n...\n`) — YAML.
/// - `+++\n…\n+++\n` — TOML.
fn split_frontmatter(source: &str) -> (usize, Option<Frontmatter>) {
    let first_line_end = source.find('\n');
    let first_line = first_line_end.map_or(source, |n| source.get(..n).unwrap_or(""));
    let trimmed_first = first_line.trim_end();
    let delimiter = match trimmed_first {
        "---" => FrontmatterDelimiter::Yaml,
        "+++" => FrontmatterDelimiter::Toml,
        _ => return (0, None),
    };
    let body_start = first_line_end.map_or(source.len(), |n| n.saturating_add(1));
    let Some(rest) = source.get(body_start..) else {
        return (0, None);
    };
    let mut cursor = 0usize;
    while cursor < rest.len() {
        let nl = rest
            .get(cursor..)
            .and_then(|s| s.find('\n'))
            .unwrap_or_else(|| rest.len().saturating_sub(cursor));
        let end_excl = cursor.saturating_add(nl);
        let line = rest.get(cursor..end_excl).unwrap_or("");
        let trimmed = line.trim_end();
        let is_close = match delimiter {
            FrontmatterDelimiter::Yaml => trimmed == "---" || trimmed == "...",
            FrontmatterDelimiter::Toml => trimmed == "+++",
        };
        if is_close {
            // Disambiguate a real frontmatter block from a leading
            // thematic break (`---`) plus a later thematic break that
            // happens to match the closing delimiter. A YAML / TOML
            // frontmatter body always contains at least one key-shaped
            // line (`key:` or `key =`); if none is present we treat
            // the source as ordinary Markdown. This is the narrowest
            // rule that preserves every real fixture while rejecting
            // the round-trip `---\n\n[a][a]\n\n---\n…` shape.
            let body_text = rest.get(..end_excl).unwrap_or("");
            if !frontmatter_body_has_key(body_text, delimiter) {
                return (0, None);
            }
            let total = body_start.saturating_add(end_excl).saturating_add(1).min(source.len());
            let text = source.get(0..total).unwrap_or("");
            return (
                total,
                Some(Frontmatter {
                    slice: TextSlice {
                        text: text.to_owned(),
                        byte_offset: 0,
                        raw_range: 0..total,
                    },
                    delimiter,
                }),
            );
        }
        cursor = end_excl.saturating_add(1);
    }
    (
        source.len(),
        Some(Frontmatter {
            slice: TextSlice {
                text: source.to_owned(),
                byte_offset: 0,
                raw_range: 0..source.len(),
            },
            delimiter,
        }),
    )
}

/// True if `body` contains at least one line shaped like a YAML key
/// (`name:`) or a TOML key (`name =`). Used by `split_frontmatter` to
/// reject false positives where the opening `---` is really a thematic
/// break and a later thematic break supplies the apparent close.
fn frontmatter_body_has_key(body: &str, delimiter: FrontmatterDelimiter) -> bool {
    let key_byte = match delimiter {
        FrontmatterDelimiter::Yaml => b':',
        FrontmatterDelimiter::Toml => b'=',
    };
    body.lines().any(|line| line_has_key(line, key_byte))
}

fn line_has_key(line: &str, key_byte: u8) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    // Optional leading whitespace.
    while i < bytes.len() && matches!(bytes.get(i).copied(), Some(b' ' | b'\t')) {
        i = i.saturating_add(1);
    }
    // First key byte: ASCII letter or underscore.
    let start = i;
    if !matches!(bytes.get(i).copied(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')) {
        return false;
    }
    i = i.saturating_add(1);
    while i < bytes.len()
        && matches!(
            bytes.get(i).copied(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.')
        )
    {
        i = i.saturating_add(1);
    }
    if i == start {
        return false;
    }
    // Optional whitespace, then the delimiter byte.
    while i < bytes.len() && matches!(bytes.get(i).copied(), Some(b' ' | b'\t')) {
        i = i.saturating_add(1);
    }
    bytes.get(i).copied() == Some(key_byte)
}

fn admonition_header_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r#"^ {0,3}(!!!|\?\?\?\+?)\s+([\w-]+)(?:\s+"([^"\n]*)")?\s*$"#))
}

/// Scan source for mkdocs-style admonition regions. A region starts
/// at a `!!! kind` / `??? kind` / `???+ kind` header line (optionally
/// followed by a `"title"` argument) and consumes contiguous following
/// lines that are blank or indented by four or more spaces (or a
/// tab). The region ends after the last such indented line; trailing
/// blank lines are not part of the region.
///
/// Headers inside a code block (`!!! note` appearing in a code
/// sample) are skipped via the `code_blocks` exclusion list.
fn scan_admonitions(source: &str, code_blocks: &[CodeBlock]) -> Vec<AdmonitionRegion> {
    let mut out = Vec::new();
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| if b == b'\n' { i.checked_add(1) } else { None }),
        )
        .collect();
    let line_end = |idx: usize| line_starts.get(idx.saturating_add(1)).copied().unwrap_or(source.len());
    let is_indented_or_blank = |s: &str| {
        let trimmed = s.trim_end_matches('\n');
        trimmed.is_empty() || trimmed.starts_with("    ") || trimmed.starts_with('\t')
    };
    let in_code_block = |range: Range<usize>| {
        code_blocks
            .iter()
            .any(|c| c.raw_range.start < range.end && range.start < c.raw_range.end)
    };
    let re = admonition_header_regex();
    let mut idx = 0usize;
    while idx < line_starts.len() {
        let start = line_starts.get(idx).copied().unwrap_or(source.len());
        let end = line_end(idx);
        let line = source.get(start..end).unwrap_or("");
        let stripped = line.trim_end_matches('\n');
        if in_code_block(start..end) || !re.is_match(stripped) {
            idx = idx.saturating_add(1);
            continue;
        }
        let mut last_content_end = end;
        let mut body_has_content = false;
        let mut j = idx.saturating_add(1);
        while j < line_starts.len() {
            let ls = line_starts.get(j).copied().unwrap_or(source.len());
            let le = line_end(j);
            let body_line = source.get(ls..le).unwrap_or("");
            let stripped_body = body_line.trim_end_matches('\n');
            let blank = stripped_body.is_empty();
            let indented = !blank && is_indented_or_blank(body_line);
            if blank {
                j = j.saturating_add(1);
                continue;
            }
            if indented {
                last_content_end = le;
                body_has_content = true;
                j = j.saturating_add(1);
                continue;
            }
            break;
        }
        if body_has_content {
            let region_range = start..last_content_end;
            let text = source.get(region_range.clone()).unwrap_or("");
            out.push(AdmonitionRegion {
                range: region_range,
                text: text.to_owned(),
            });
            // Move past the last consumed line.
            idx = j;
        } else {
            // Header with no indented body is not an admonition.
            idx = idx.saturating_add(1);
        }
    }
    out
}

fn suppression_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Order matters: `allow-next-line` must precede `allow`, and
    // `disable-all` / `enable-all` must precede their bare forms,
    // because regex alternation is greedy left-to-right.
    // Leading whitespace is space-only: tabs do not count as
    // indentation (CommonMark §2.2; the mdformat-mkdocs tab bug is
    // the negative reference).
    RE.get_or_init(|| {
        compile_static(
            r"^ {0,3}<!--\s*mdwright:\s*(?P<kind>allow-next-line|allow|disable-all|enable-all|disable|enable)(?:[ \t]+(?P<names>[\w\-,\s]+?))?\s*-->\s*$",
        )
    })
}

/// Parse suppression directives from HTML comments. Only block-level
/// HTML is consulted — pulldown-cmark already distinguishes a comment
/// on its own line (`HtmlBlock`) from an inline comment (`InlineHtml`),
/// which gives us the "own source line" requirement for free.
fn scan_suppressions(html_blocks: &[HtmlBlock]) -> Vec<Suppression> {
    let mut out = Vec::new();
    let re = suppression_regex();
    for block in html_blocks {
        let trimmed = block.text.trim_end();
        let Some(caps) = re.captures(trimmed) else {
            continue;
        };
        let Some(kind_match) = caps.name("kind") else {
            continue;
        };
        let kind = match kind_match.as_str() {
            "allow" => SuppressionKind::Allow {
                scope: AllowScope::Block,
            },
            "allow-next-line" => SuppressionKind::Allow {
                scope: AllowScope::NextLine,
            },
            "disable" | "disable-all" => SuppressionKind::Disable,
            "enable" | "enable-all" => SuppressionKind::Enable,
            _ => continue,
        };
        let rules: Vec<String> = caps
            .name("names")
            .map_or("", |m| m.as_str())
            .split([',', ' ', '\t'])
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        // `allow` and `allow-next-line` require explicit names; a bare
        // form is malformed syntax and is silently dropped. `disable`
        // / `enable` accept an empty name list (= "every known rule").
        if matches!(kind, SuppressionKind::Allow { .. }) && rules.is_empty() {
            continue;
        }
        out.push(Suppression {
            kind,
            rules,
            raw_range: block.raw_range.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow};

    use super::Ir;

    #[test]
    fn prose_chunks_include_backslash_escapes() {
        let ir = Ir::parse_str(r"a \_b\_ c");
        let texts: Vec<&str> = ir.prose_chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(
            texts.iter().any(|t| t.contains(r"\_")),
            "prose chunks should preserve `\\_`: {texts:?}"
        );
    }

    #[test]
    fn fenced_code_excluded_from_prose() {
        let src = "before\n```\nx \\_y\\_ z\n```\nafter \\_outside\\_\n";
        let ir = Ir::parse_str(src);
        // No chunk should contain the code-block body.
        for c in &ir.prose_chunks {
            assert!(!c.text.contains("\\_y"), "prose chunk leaked code body: {:?}", c.text);
        }
        // The escapes outside the fence ARE visible: at least one
        // chunk must contain `\_` and at least one must contain
        // `outside`. (Text events split at escape boundaries, so the
        // full literal `\_outside\_` is spread across multiple chunks.)
        let texts: Vec<&str> = ir.prose_chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("\\_")), "no chunk has `\\_`: {texts:?}");
        assert!(
            texts.iter().any(|t| t.contains("outside")),
            "no chunk has `outside`: {texts:?}"
        );
        assert_eq!(ir.code_blocks.len(), 1);
    }

    #[test]
    fn inline_code_strips_fences() -> Result<()> {
        let ir = Ir::parse_str("see `foo_bar` here\n");
        assert_eq!(ir.inline_codes.len(), 1);
        let code = ir.inline_codes.first().ok_or_else(|| anyhow!("missing"))?;
        assert_eq!(code.text, "foo_bar");
        Ok(())
    }

    #[test]
    fn frontmatter_split() -> Result<()> {
        let src = "---\ntitle: T\n---\nbody text\n";
        let ir = Ir::parse_str(src);
        let fm = ir.frontmatter.as_ref().ok_or_else(|| anyhow!("frontmatter"))?;
        assert_eq!(fm.delimiter, super::FrontmatterDelimiter::Yaml);
        let body_chunks: Vec<&str> = ir.prose_chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(body_chunks.iter().any(|t| t == &"body text"));
        Ok(())
    }

    #[test]
    fn frontmatter_toml_split() -> Result<()> {
        let src = "+++\ntitle = \"T\"\n+++\nbody text\n";
        let ir = Ir::parse_str(src);
        let fm = ir.frontmatter.as_ref().ok_or_else(|| anyhow!("frontmatter"))?;
        assert_eq!(fm.delimiter, super::FrontmatterDelimiter::Toml);
        let body_chunks: Vec<&str> = ir.prose_chunks.iter().map(|c| c.text.as_str()).collect();
        assert!(body_chunks.iter().any(|t| t == &"body text"));
        Ok(())
    }

    #[test]
    fn admonition_scan_basic() -> Result<()> {
        let src = "!!! note\n    hello\n    world\n\nafter\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.admonitions.len(), 1);
        let region = ir.admonitions.first().ok_or_else(|| anyhow!("region"))?;
        assert_eq!(region.text, "!!! note\n    hello\n    world\n");
        Ok(())
    }

    #[test]
    fn admonition_scan_with_title_and_collapsible() {
        let src = "??? warning \"Be careful\"\n    body line\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.admonitions.len(), 1);
    }

    #[test]
    fn admonition_scan_inside_code_block_skipped() {
        let src = "```\n!!! note\n    body\n```\n";
        let ir = Ir::parse_str(src);
        assert!(ir.admonitions.is_empty());
    }

    #[test]
    fn headings_trimmed_and_levelled() {
        let ir = Ir::parse_str("# One\n\n## Two ##\n\n### Three\n");
        assert_eq!(ir.headings.len(), 3);
        let texts: Vec<(&str, u32)> = ir.headings.iter().map(|h| (h.text.as_str(), h.level)).collect();
        assert_eq!(texts, vec![("One", 1), ("Two", 2), ("Three", 3)]);
    }

    #[test]
    fn list_groups_record_markers() -> Result<()> {
        let src = "- one\n- two\n* three\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.list_groups.len(), 2);
        let g1 = ir.list_groups.first().ok_or_else(|| anyhow!("first list"))?;
        assert!(!g1.ordered);
        let markers: Vec<u8> = g1.items.iter().map(|i| i.marker_byte).collect();
        assert_eq!(markers, vec![b'-', b'-']);
        let g2 = ir.list_groups.get(1).ok_or_else(|| anyhow!("second list"))?;
        let item = g2.items.first().ok_or_else(|| anyhow!("item"))?;
        assert_eq!(item.marker_byte, b'*');
        Ok(())
    }

    #[test]
    fn link_defs_scanned() -> Result<()> {
        let src = "[bar]: https://example.com\n\nSee [ref][bar].\n";
        let ir = Ir::parse_str(src);
        let target = ir.refs.iter().next().ok_or_else(|| anyhow!("expected one target"))?;
        assert_eq!(target.label_raw, "bar");
        assert_eq!(target.dest, "https://example.com");
        Ok(())
    }

    #[test]
    fn link_defs_skipped_inside_code_block() {
        let src = "```\n[bar]: https://example.com\n```\n";
        let ir = Ir::parse_str(src);
        assert!(ir.refs.is_empty());
    }

    #[test]
    fn inline_html_collected() {
        let src = "before <span>x</span> after\n";
        let ir = Ir::parse_str(src);
        assert!(ir.inline_html.iter().any(|h| h.text == "<span>"));
        assert!(ir.inline_html.iter().any(|h| h.text == "</span>"));
    }

    #[test]
    fn code_block_info_string() -> Result<()> {
        let src = "```rust\nfn x() {}\n```\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.code_blocks.len(), 1);
        let cb = ir.code_blocks.first().ok_or_else(|| anyhow!("cb"))?;
        assert_eq!(cb.info, "rust");
        assert!(cb.fenced);
        Ok(())
    }

    use super::{AllowScope, SuppressionKind};

    #[test]
    fn suppression_allow_parses() -> Result<()> {
        let src = "<!-- mdwright: allow heading-punctuation -->\n# Title.\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.suppressions.len(), 1);
        let s = ir.suppressions.first().ok_or_else(|| anyhow!("first"))?;
        assert_eq!(
            s.kind,
            SuppressionKind::Allow {
                scope: AllowScope::Block
            }
        );
        assert_eq!(s.rules, vec!["heading-punctuation"]);
        Ok(())
    }

    #[test]
    fn suppression_allow_next_line_parses() -> Result<()> {
        let src = "<!-- mdwright: allow-next-line trailing-whitespace -->\nfoo \n";
        let ir = Ir::parse_str(src);
        let s = ir.suppressions.first().ok_or_else(|| anyhow!("first"))?;
        assert_eq!(
            s.kind,
            SuppressionKind::Allow {
                scope: AllowScope::NextLine
            }
        );
        Ok(())
    }

    #[test]
    fn suppression_multiple_rules_parses() -> Result<()> {
        let src = "<!-- mdwright: allow rule-a, rule-b, rule-c -->\nbody\n";
        let ir = Ir::parse_str(src);
        let s = ir.suppressions.first().ok_or_else(|| anyhow!("first"))?;
        assert_eq!(s.rules, vec!["rule-a", "rule-b", "rule-c"]);
        Ok(())
    }

    #[test]
    fn suppression_disable_enable_parse() -> Result<()> {
        let src = "<!-- mdwright: disable bare-url -->\n\nfoo\n\n<!-- mdwright: enable bare-url -->\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.suppressions.len(), 2);
        let first = ir.suppressions.first().ok_or_else(|| anyhow!("first"))?;
        let second = ir.suppressions.get(1).ok_or_else(|| anyhow!("second"))?;
        assert_eq!(first.kind, SuppressionKind::Disable);
        assert_eq!(second.kind, SuppressionKind::Enable);
        Ok(())
    }

    #[test]
    fn suppression_disable_all_alias_parses() -> Result<()> {
        let src = "<!-- mdwright: disable-all -->\nfoo\n";
        let ir = Ir::parse_str(src);
        let s = ir.suppressions.first().ok_or_else(|| anyhow!("first"))?;
        assert_eq!(s.kind, SuppressionKind::Disable);
        assert!(s.rules.is_empty());
        Ok(())
    }

    #[test]
    fn suppression_bare_allow_rejected() {
        // `allow` with no names is malformed; silently dropped.
        let src = "<!-- mdwright: allow -->\n# Title\n";
        let ir = Ir::parse_str(src);
        assert!(ir.suppressions.is_empty());
    }

    #[test]
    fn suppression_inline_html_ignored() {
        // A comment inside a paragraph is InlineHtml, not HtmlBlock,
        // so the scanner doesn't see it. This preserves the "own
        // source line" requirement.
        let src = "Some text <!-- mdwright: allow bare-url --> more text.\n";
        let ir = Ir::parse_str(src);
        assert!(ir.suppressions.is_empty());
    }

    #[test]
    fn suppression_with_indent_parses() -> Result<()> {
        // Up to three spaces of indentation are allowed.
        let src = "   <!-- mdwright: allow heading-punctuation -->\n# Title.\n";
        let ir = Ir::parse_str(src);
        let s = ir.suppressions.first().ok_or_else(|| anyhow!("first"))?;
        assert_eq!(s.rules, vec!["heading-punctuation"]);
        Ok(())
    }

    use super::compute_transparent_runs;

    #[test]
    fn transparent_runs_for_blockquote_continuation() {
        // Two `>` lines yield one transparent run per line covering
        // the `> ` prefix.
        let src = "> a\n> b\n";
        let bq = 0..src.len();
        let runs = compute_transparent_runs(src, std::slice::from_ref(&bq), &[]);
        assert_eq!(runs, vec![0..2, 4..6]);
    }

    #[test]
    fn transparent_runs_for_nested_blockquote() {
        // `> > a / > > b`: each line gets one run combining both
        // levels of nesting (`> > ` is 4 bytes).
        let src = "> > a\n> > b\n";
        let outer = 0..src.len();
        let inner = 2..src.len();
        let runs = compute_transparent_runs(src, &[outer, inner], &[]);
        assert_eq!(runs, vec![0..4, 6..10]);
    }

    #[test]
    fn transparent_runs_for_list_item_continuation() {
        // `1. a\n   b\n`: line 1 is the marker line (no run); line 2
        // is a continuation line whose 3-space indent is stripped.
        let src = "1. a\n   b\n";
        let item = (0..src.len(), 3);
        let runs = compute_transparent_runs(src, &[], &[item]);
        assert_eq!(runs, vec![5..8]);
    }

    #[test]
    fn transparent_runs_empty_for_plain_paragraph() {
        // No container context → no transparent runs (fast path).
        let src = "hello\nworld\n";
        let runs = compute_transparent_runs(src, &[], &[]);
        assert!(runs.is_empty(), "expected empty: {runs:?}");
    }
}
