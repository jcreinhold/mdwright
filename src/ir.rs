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

#![allow(dead_code)]
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
    pub(crate) abbreviations: Vec<AbbreviationRegion>,
    pub(crate) block_attrs: Vec<BlockAttrRegion>,
    pub(crate) directives: Vec<DirectiveRegion>,
    pub(crate) comments: Vec<CommentRegion>,
    pub(crate) inline_overlays: Vec<InlineOverlayRegion>,
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

/// One `*[TERM]: definition` abbreviation declaration, recognised by
/// [`scan_abbreviations`] when the `abbreviation_lists` extension is
/// enabled. Scan-and-preserve overlay: the formatter emits the
/// region's bytes verbatim and skips the tree paragraph that pulldown
/// built from the same line(s).
///
/// `term` and `definition` are subranges inside `range` covering the
/// abbreviation key (between the `[` and `]`) and the body (after the
/// `:` and whitespace), respectively — kept for future lint rules.
#[derive(Clone, Debug)]
pub(crate) struct AbbreviationRegion {
    pub(crate) range: Range<usize>,
    #[allow(dead_code)]
    pub(crate) term: Range<usize>,
    #[allow(dead_code)]
    pub(crate) definition: Range<usize>,
}

/// One `{ #id .class key=val }` trailer attached to the previous
/// block (paragraph, image, or fenced block), recognised by
/// [`scan_block_attrs`] when the `block_attribute_lists` extension is
/// enabled. Scan-and-preserve overlay: the formatter emits the trailer
/// bytes verbatim alongside the preceding block.
#[derive(Clone, Debug)]
pub(crate) struct BlockAttrRegion {
    pub(crate) range: Range<usize>,
    #[allow(dead_code)]
    pub(crate) target_range: Range<usize>,
    #[allow(dead_code)]
    pub(crate) attrs_range: Range<usize>,
}

/// One `MyST` / `Pandoc` directive container detected by [`scan_directives`].
///
/// `range` covers opener line start through closer line end inclusive
/// (with the trailing newline if present). Nested directives sit inside
/// the outer region's bytes — only the outermost is recorded; the
/// verbatim emit reproduces inner directives implicitly.
#[derive(Clone, Debug)]
pub(crate) struct DirectiveRegion {
    pub(crate) range: Range<usize>,
    #[allow(dead_code)]
    pub(crate) style: DirectiveStyle,
    #[allow(dead_code)]
    pub(crate) colon_count: u8,
    #[allow(dead_code)]
    pub(crate) opener_line: Range<usize>,
    #[allow(dead_code)]
    pub(crate) closer_line: Range<usize>,
}

/// The three syntactic flavours of directive opener mdwright recognises.
/// Each is independently gated:
/// [`crate::config::MystOptions::directive_containers`] for `MystBrace`,
/// [`crate::config::PandocOptions::fenced_divs`] for `PandocAttrs`, and
/// [`crate::config::PandocOptions::short_form_divs`] for `PandocShort`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum DirectiveStyle {
    /// `:::{name}` (`MyST` brace form, with optional `:KEY: value` option
    /// lines).
    MystBrace,
    /// `::: {.cls}` (`Pandoc` attribute form).
    PandocAttrs,
    /// `:::name` (`Pandoc` short form).
    PandocShort,
}

/// One `MyST` `%` line comment detected by [`scan_comments`].
///
/// `range` covers the comment line including the trailing newline.
#[derive(Clone, Debug)]
pub(crate) struct CommentRegion {
    pub(crate) range: Range<usize>,
}

/// One inline overlay region (role, substitution, or `Pandoc` inline
/// attribute span) detected by [`scan_inline_overlays`].
///
/// `range` covers the entire overlay literal; `kind` carries the
/// per-flavour offsets for future lint rules. The inline-overlay
/// formatter at `src/format/inline.rs::apply_inline_overlay` consults
/// only `range`, emitting the slice verbatim and skipping any tree
/// nodes whose `raw_range` falls inside.
#[derive(Clone, Debug)]
pub(crate) struct InlineOverlayRegion {
    pub(crate) range: Range<usize>,
    #[allow(dead_code)]
    pub(crate) kind: InlineOverlayKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InlineOverlayKind {
    /// `MyST` inline role: `` {role}`payload` ``.
    Role {
        #[allow(dead_code)]
        name_range: Range<usize>,
        #[allow(dead_code)]
        payload_range: Range<usize>,
    },
    /// `MyST` substitution reference: `{{name}}`.
    Substitution {
        #[allow(dead_code)]
        name_range: Range<usize>,
    },
    /// `Pandoc` inline attribute span: `[content]{.cls}`.
    PandocSpan {
        #[allow(dead_code)]
        content_range: Range<usize>,
        #[allow(dead_code)]
        attrs_range: Range<usize>,
    },
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
        let abbreviations = scan_abbreviations(
            source,
            &builder.code_blocks,
            &builder.inline_codes,
            &builder.html_blocks,
            &builder.inline_html,
        );
        let tree = tree_builder.finalize(&refs);
        let block_attrs = scan_block_attrs(
            source,
            &tree,
            &builder.code_blocks,
            &builder.inline_codes,
            &builder.html_blocks,
            &builder.inline_html,
        );
        let directives = scan_directives(
            source,
            &builder.code_blocks,
            &builder.inline_codes,
            &builder.html_blocks,
            &builder.inline_html,
        );
        let comments = scan_comments(
            source,
            &builder.code_blocks,
            &builder.inline_codes,
            &builder.html_blocks,
            &builder.inline_html,
        );
        let inline_overlays = scan_inline_overlays(
            source,
            &builder.code_blocks,
            &builder.inline_codes,
            &builder.html_blocks,
            &builder.inline_html,
            &math_regions,
            &directives,
        );

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
            abbreviations,
            block_attrs,
            directives,
            comments,
            inline_overlays,
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
    // No closing delimiter — the opener is a thematic break (`---`)
    // or just plain text (`+++`), not a frontmatter fence. Returning
    // the whole source as frontmatter would byte-preserve the document
    // by short-circuiting the tree builder, which masks the structural
    // emit's loose-list normalisation for any document that happens to
    // start with `---\n`. Treat as no frontmatter and let pulldown
    // reparse the opener.
    let _ = delimiter;
    (0, None)
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

fn abbreviation_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"^ {0,3}\*\[([^\]\n]+)\]:[ \t]+(.+?)[ \t]*$"))
}

/// Scan source for `*[TERM]: definition` abbreviation declarations.
/// Single-line per declaration; no continuation lines (matches
/// python-markdown / mdformat-mkdocs behaviour). Declarations inside
/// code spans / blocks / HTML blocks / inline HTML are skipped.
///
/// The scan is unconditional at parse time; the overlay arm in
/// `format::block::pretty_block_sequence` gates the actual verbatim
/// emission on [`crate::config::ExtensionOptions::abbreviation_lists`]
/// so the parsed regions can also feed future lint rules even when the
/// formatter overlay is off.
fn scan_abbreviations(
    source: &str,
    code_blocks: &[CodeBlock],
    inline_codes: &[InlineCode],
    html_blocks: &[HtmlBlock],
    inline_html: &[InlineHtml],
) -> Vec<AbbreviationRegion> {
    // Fast path: the abbreviation header always begins with `*[`. A
    // single substring scan rules out documents that contain none —
    // the common case for non-mkdocs corpora — without paying the
    // per-line regex cost.
    if !source.contains("*[") {
        return Vec::new();
    }
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
    let excluded = |range: Range<usize>| -> bool {
        let overlaps = |r: &Range<usize>| r.start < range.end && range.start < r.end;
        code_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_codes.iter().any(|c| overlaps(&c.raw_range))
            || html_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_html.iter().any(|c| overlaps(&c.raw_range))
    };
    let re = abbreviation_regex();
    for idx in 0..line_starts.len() {
        let start = line_starts.get(idx).copied().unwrap_or(source.len());
        let end = line_end(idx);
        if excluded(start..end) {
            continue;
        }
        let line = source.get(start..end).unwrap_or("");
        let stripped = line.trim_end_matches('\n');
        let Some(caps) = re.captures(stripped) else {
            continue;
        };
        let Some(term_m) = caps.get(1) else { continue };
        let Some(def_m) = caps.get(2) else { continue };
        out.push(AbbreviationRegion {
            range: start..end,
            term: start.saturating_add(term_m.start())..start.saturating_add(term_m.end()),
            definition: start.saturating_add(def_m.start())..start.saturating_add(def_m.end()),
        });
    }
    out
}

fn block_attr_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"^[ \t]*(\{[^}\n]+\})[ \t]*$"))
}

/// Scan source for `{ #id .class key=val }` lines that immediately
/// follow a block element (paragraph, image, fenced code block,
/// blockquote, list, table). The trailer line itself is the
/// `BlockAttrRegion`; the preceding block's `raw_range` is recorded
/// as `target_range` so a future lint rule can verify the attachment.
///
/// Like [`scan_abbreviations`], the scan is unconditional; the overlay
/// arm gates emission on
/// [`crate::config::ExtensionOptions::block_attribute_lists`].
fn scan_block_attrs(
    source: &str,
    tree: &Tree,
    code_blocks: &[CodeBlock],
    inline_codes: &[InlineCode],
    html_blocks: &[HtmlBlock],
    inline_html: &[InlineHtml],
) -> Vec<BlockAttrRegion> {
    // Fast path: the trailer always begins with `{` somewhere in the
    // source. The byte-search is much cheaper than walking every
    // top-level block's last line through the regex.
    if !source.contains('{') {
        return Vec::new();
    }
    let mut out = Vec::new();
    let excluded = |range: &Range<usize>| -> bool {
        let overlaps = |r: &Range<usize>| r.start < range.end && range.start < r.end;
        code_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_codes.iter().any(|c| overlaps(&c.raw_range))
            || html_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_html.iter().any(|c| overlaps(&c.raw_range))
    };
    let re = block_attr_regex();
    // For each top-level block, check whether its last line is a
    // `{...}` trailer. Pulldown bundles such trailers into the
    // preceding paragraph (it does not recognise the attribute-list
    // extension), so the trailer lives at the tail of the parent
    // block's raw_range. When found, the `range` field covers the
    // whole parent block (body + trailer) so the overlay arm emits
    // the unit verbatim; `target_range` and `attrs_range` split the
    // two halves for any future lint that wants to verify them.
    for cid in tree.children(tree.root()) {
        let Some(node) = tree.node(cid) else { continue };
        let raw_range = node.raw_range.clone();
        // A standalone trailer line (block with only the trailer) is
        // not an attribute attachment — leave it as plain text.
        let raw = source.get(raw_range.clone()).unwrap_or("");
        let trimmed = raw.trim_end_matches('\n');
        // Locate the start of the last line.
        let last_line_offset = trimmed.rfind('\n').map_or(0, |n| n.saturating_add(1));
        if last_line_offset == 0 {
            continue;
        }
        let last_line = trimmed.get(last_line_offset..).unwrap_or("");
        let Some(caps) = re.captures(last_line) else {
            continue;
        };
        let Some(braces) = caps.get(1) else { continue };
        let attrs_start = raw_range.start.saturating_add(last_line_offset);
        let attrs_end = raw_range.start.saturating_add(trimmed.len());
        let attrs_range = attrs_start..attrs_end;
        if excluded(&attrs_range) {
            continue;
        }
        out.push(BlockAttrRegion {
            range: raw_range.clone(),
            target_range: raw_range.start..attrs_start,
            attrs_range: attrs_start.saturating_add(braces.start())..attrs_start.saturating_add(braces.end()),
        });
    }
    out
}

fn directive_opener_regex() -> &'static Regex {
    // `:::` (≥ 3 colons) optionally followed by either:
    //   - `{name}` or `{ .cls #id }` (`MyST` brace / `Pandoc` attrs),
    //     plus an optional argument tail (e.g. `:::{figure} ./img.png`);
    //   - a bare identifier `name` (`Pandoc` short form), plus an
    //     optional argument tail;
    //   - nothing (anonymous opener — left to the disambiguator).
    // Leading whitespace permitted up to 3 spaces.
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        compile_static(
            r"^ {0,3}(?P<colons>:{3,})(?:[ \t]*(?P<brace>\{[^}\n]*\})[ \t]*[^\n]*|[ \t]*(?P<short>[A-Za-z][\w-]*)[ \t]*[^\n]*)?[ \t]*$",
        )
    })
}

fn directive_closer_regex() -> &'static Regex {
    // Closer: only colons (no name / no attrs), ≥ the opener's count.
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"^ {0,3}(?P<colons>:{3,})[ \t]*$"))
}

/// Scan source for `MyST` / `Pandoc` directive containers.
///
/// Recognises three opener flavours: `MyST` `:::{name}`, `Pandoc`
/// `::: {.cls}`, and `Pandoc` `:::name`. An opener of *n* colons is
/// matched by the next colon-only line of count ≥ *n*; the recorded
/// region spans opener-line-start through closer-line-end inclusive.
///
/// Nested directives sit inside the outer region's bytes — only the
/// outermost is recorded; the verbatim emit at
/// `format::block::pretty_block_sequence` reproduces inner directives
/// implicitly.
///
/// The scan is unconditional at parse time; the overlay arm gates
/// emission on [`crate::config::MystOptions::directive_containers`]
/// (plus the `Pandoc` style toggles).
fn scan_directives(
    source: &str,
    code_blocks: &[CodeBlock],
    inline_codes: &[InlineCode],
    html_blocks: &[HtmlBlock],
    inline_html: &[InlineHtml],
) -> Vec<DirectiveRegion> {
    if !source.contains(":::") {
        return Vec::new();
    }
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| if b == b'\n' { i.checked_add(1) } else { None }),
        )
        .collect();
    let line_end = |idx: usize| line_starts.get(idx.saturating_add(1)).copied().unwrap_or(source.len());
    let excluded = |range: Range<usize>| -> bool {
        let overlaps = |r: &Range<usize>| r.start < range.end && range.start < r.end;
        code_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_codes.iter().any(|c| overlaps(&c.raw_range))
            || html_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_html.iter().any(|c| overlaps(&c.raw_range))
    };
    let opener = directive_opener_regex();
    let closer = directive_closer_regex();

    let mut out = Vec::new();
    let mut idx = 0usize;
    while idx < line_starts.len() {
        let start = line_starts.get(idx).copied().unwrap_or(source.len());
        let end = line_end(idx);
        if excluded(start..end) {
            idx = idx.saturating_add(1);
            continue;
        }
        let line = source.get(start..end).unwrap_or("");
        let stripped = line.trim_end_matches('\n');
        // Closer-style lines (colon-only) cannot themselves be openers;
        // skip them here so they're available for a parent opener's
        // close pass. The opener regex's optional `{…}` / short suffix
        // means a colon-only line would otherwise be ambiguous; the
        // structural rule below disambiguates by requiring a brace or
        // short name for `MystBrace` / `PandocAttrs` / `PandocShort` and
        // marks the bare form as an anonymous opener only when no
        // matching closer follows.
        let Some(opener_caps) = opener.captures(stripped) else {
            idx = idx.saturating_add(1);
            continue;
        };
        let colon_run = opener_caps.name("colons").map_or("", |m| m.as_str());
        let style = if opener_caps.name("brace").is_some() {
            // Brace form. Decide MystBrace vs PandocAttrs by inspecting
            // the first non-whitespace char inside the braces: `{name}`
            // is `MyST`, `{.cls}` / `{#id}` is `Pandoc`.
            let brace = opener_caps.name("brace").map_or("", |m| m.as_str());
            let inner = brace.trim_start_matches('{').trim_end_matches('}').trim();
            if inner.starts_with('.') || inner.starts_with('#') {
                DirectiveStyle::PandocAttrs
            } else {
                DirectiveStyle::MystBrace
            }
        } else if opener_caps.name("short").is_some() {
            DirectiveStyle::PandocShort
        } else {
            // Bare `:::` on a line by itself — cannot tell opener from
            // closer in isolation; skip.
            idx = idx.saturating_add(1);
            continue;
        };
        let count = u8::try_from(colon_run.len()).unwrap_or(u8::MAX);

        // Find a matching closer (colon-only, count ≥ opener count).
        let mut search = idx.saturating_add(1);
        let mut matched: Option<usize> = None;
        while search < line_starts.len() {
            let s = line_starts.get(search).copied().unwrap_or(source.len());
            let e = line_end(search);
            if excluded(s..e) {
                search = search.saturating_add(1);
                continue;
            }
            let cl = source.get(s..e).unwrap_or("").trim_end_matches('\n');
            if let Some(c_caps) = closer.captures(cl) {
                let c_run = c_caps.name("colons").map_or("", |m| m.as_str());
                if c_run.len() >= colon_run.len() {
                    matched = Some(search);
                    break;
                }
            }
            search = search.saturating_add(1);
        }
        let Some(closer_idx) = matched else {
            idx = idx.saturating_add(1);
            continue;
        };
        let closer_start = line_starts.get(closer_idx).copied().unwrap_or(source.len());
        let closer_end = line_end(closer_idx);
        out.push(DirectiveRegion {
            range: start..closer_end,
            style,
            colon_count: count,
            opener_line: start..end,
            closer_line: closer_start..closer_end,
        });
        idx = closer_idx.saturating_add(1);
    }
    out
}

/// Scan source for `MyST` `%` line comments.
///
/// Recognises a line whose first non-whitespace byte is `%` (outside
/// fenced code / HTML blocks / inline code / inline HTML). One line
/// per region. Subject to
/// [`crate::config::MystOptions::comments`] gating at the overlay arm.
fn scan_comments(
    source: &str,
    code_blocks: &[CodeBlock],
    inline_codes: &[InlineCode],
    html_blocks: &[HtmlBlock],
    inline_html: &[InlineHtml],
) -> Vec<CommentRegion> {
    if !source.contains('%') {
        return Vec::new();
    }
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter_map(|(i, b)| if b == b'\n' { i.checked_add(1) } else { None }),
        )
        .collect();
    let line_end = |idx: usize| line_starts.get(idx.saturating_add(1)).copied().unwrap_or(source.len());
    let excluded = |range: Range<usize>| -> bool {
        let overlaps = |r: &Range<usize>| r.start < range.end && range.start < r.end;
        code_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_codes.iter().any(|c| overlaps(&c.raw_range))
            || html_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_html.iter().any(|c| overlaps(&c.raw_range))
    };
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    for idx in 0..line_starts.len() {
        let start = line_starts.get(idx).copied().unwrap_or(source.len());
        // Quick byte scan past leading spaces / tabs without slicing
        // or allocating; bail early on the first non-whitespace byte
        // that isn't `%`. This is on the hot path for any corpus that
        // contains `%` somewhere (LaTeX math, percent signs in prose).
        let mut p = start;
        while bytes.get(p).is_some_and(|&b| b == b' ' || b == b'\t') {
            p = p.saturating_add(1);
        }
        if bytes.get(p) != Some(&b'%') {
            continue;
        }
        let end = line_end(idx);
        if excluded(start..end) {
            continue;
        }
        out.push(CommentRegion { range: start..end });
    }
    out
}

fn inline_role_regex() -> &'static Regex {
    // `{name}` immediately followed by one or more backticks. We only
    // anchor the opener here; the backtick run + payload + closing
    // run are matched programmatically because the run length is
    // determined dynamically (CommonMark code-span pairing).
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"\{(?P<name>[A-Za-z][\w-]*)\}(?P<ticks>`+)"))
}

fn inline_substitution_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"\{\{(?P<name>[A-Za-z][\w-]*)\}\}"))
}

/// Scan source for inline overlay constructs: `MyST` inline roles
/// (`` {role}`payload` ``), `MyST` substitution references (`{{name}}`),
/// and `Pandoc` inline attribute spans (`[content]{.cls}`).
///
/// Excludes regions inside fenced code, inline code, HTML blocks /
/// inline HTML, math regions, and block-level directive regions
/// (whose verbatim emit owns the bytes outright).
///
/// Gated per-kind at the inline overlay site:
/// [`crate::config::MystOptions::inline_roles`],
/// [`crate::config::MystOptions::substitution_references`],
/// [`crate::config::PandocOptions::inline_attribute_spans`].
fn scan_inline_overlays(
    source: &str,
    code_blocks: &[CodeBlock],
    inline_codes: &[InlineCode],
    html_blocks: &[HtmlBlock],
    inline_html: &[InlineHtml],
    math_regions: &[MathRegion],
    directives: &[DirectiveRegion],
) -> Vec<InlineOverlayRegion> {
    // Fast path: every overlay shape starts with `{` or `[`; if the
    // source has neither, skip the whole pass.
    if !source.contains('{') && !source.contains('[') {
        return Vec::new();
    }
    let excluded = |range: &Range<usize>| -> bool {
        let overlaps = |r: &Range<usize>| r.start < range.end && range.start < r.end;
        code_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_codes.iter().any(|c| overlaps(&c.raw_range))
            || html_blocks.iter().any(|c| overlaps(&c.raw_range))
            || inline_html.iter().any(|c| overlaps(&c.raw_range))
            || math_regions.iter().any(|m| overlaps(&m.range))
            || directives.iter().any(|d| overlaps(&d.range))
    };
    let mut out: Vec<InlineOverlayRegion> = Vec::new();

    // Pass 1: substitutions. Scan first so `{{name}}` is not eaten by
    // the role pattern. Fast-path on `{{` — without it the regex still
    // scans every byte on documents that have no substitutions.
    if source.contains("{{") {
        for caps in inline_substitution_regex().captures_iter(source) {
            let Some(m) = caps.get(0) else { continue };
            let r = m.start()..m.end();
            if excluded(&r) {
                continue;
            }
            let Some(name_m) = caps.name("name") else { continue };
            out.push(InlineOverlayRegion {
                range: r,
                kind: InlineOverlayKind::Substitution {
                    name_range: name_m.start()..name_m.end(),
                },
            });
        }
    }

    // Pass 2: inline roles. The role opener is `{name}` immediately
    // followed by one or more backticks; the payload is closed by a
    // backtick run of the same length (CommonMark code-span pairing).
    // Fast-path: a role requires both `{` and a backtick.
    if source.contains('{') && source.contains('`') {
        let role_re = inline_role_regex();
        for caps in role_re.captures_iter(source) {
            let Some(opener) = caps.get(0) else { continue };
            let Some(name_m) = caps.name("name") else { continue };
            let Some(ticks_m) = caps.name("ticks") else { continue };
            let tick_len = ticks_m.end().saturating_sub(ticks_m.start());
            let payload_start = ticks_m.end();
            // Find a matching backtick run of exactly `tick_len` (CommonMark
            // §6.1: pairing on equal run length).
            let needle = "`".repeat(tick_len);
            let rest = source.get(payload_start..).unwrap_or("");
            let mut search_from = 0usize;
            let payload_end_local = loop {
                let Some(rel) = rest.get(search_from..).and_then(|s| s.find(&needle)) else {
                    break None;
                };
                let abs = search_from.saturating_add(rel);
                // Ensure this is a run of exactly tick_len (not a longer run).
                let before_ok = abs == 0 || rest.as_bytes().get(abs.saturating_sub(1)) != Some(&b'`');
                let after_idx = abs.saturating_add(tick_len);
                let after_ok = rest.as_bytes().get(after_idx) != Some(&b'`');
                if before_ok && after_ok {
                    break Some(abs);
                }
                search_from = abs.saturating_add(tick_len);
            };
            let Some(payload_end_local) = payload_end_local else {
                continue;
            };
            let payload_end = payload_start.saturating_add(payload_end_local);
            let region_end = payload_end.saturating_add(tick_len);
            let r = opener.start()..region_end;
            if excluded(&r) {
                continue;
            }
            out.push(InlineOverlayRegion {
                range: r,
                kind: InlineOverlayKind::Role {
                    name_range: name_m.start()..name_m.end(),
                    payload_range: payload_start..payload_end,
                },
            });
        }
    }

    // Pass 3: `Pandoc` inline attribute spans `[content]{.cls}`. Skip if
    // followed by `(` (CommonMark link). Fast-path on `]{` substring
    // presence — corpora with lots of links but no Pandoc spans pay an
    // O(n) walk otherwise, dominating per-document scanner cost.
    if !source.contains("]{") {
        out.sort_by_key(|r| r.range.start);
        return out;
    }
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes.get(i) != Some(&b'[') {
            i = i.saturating_add(1);
            continue;
        }
        // Find balanced `]` on the same line.
        let mut depth = 1i32;
        let mut j = i.saturating_add(1);
        let mut closed = false;
        while j < bytes.len() {
            match bytes.get(j) {
                Some(&b'\n') => break,
                Some(&b'\\') => {
                    j = j.saturating_add(2);
                    continue;
                }
                Some(&b'[') => depth = depth.saturating_add(1),
                Some(&b']') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        closed = true;
                        break;
                    }
                }
                _ => {}
            }
            j = j.saturating_add(1);
        }
        if !closed {
            i = i.saturating_add(1);
            continue;
        }
        let content_start = i.saturating_add(1);
        let content_end = j;
        let after_bracket = j.saturating_add(1);
        // Must be immediately followed by `{`; if followed by `(`, it's
        // a CommonMark link — skip.
        let Some(&first) = bytes.get(after_bracket) else {
            i = j.saturating_add(1);
            continue;
        };
        if first == b'(' {
            i = j.saturating_add(1);
            continue;
        }
        if first != b'{' {
            i = j.saturating_add(1);
            continue;
        }
        let attrs_open = after_bracket;
        let mut k = attrs_open.saturating_add(1);
        let mut attrs_close: Option<usize> = None;
        while k < bytes.len() {
            match bytes.get(k) {
                Some(&b'\n') => break,
                Some(&b'}') => {
                    attrs_close = Some(k);
                    break;
                }
                _ => {}
            }
            k = k.saturating_add(1);
        }
        let Some(attrs_close) = attrs_close else {
            i = j.saturating_add(1);
            continue;
        };
        let r = i..attrs_close.saturating_add(1);
        if excluded(&r) {
            i = j.saturating_add(1);
            continue;
        }
        // Avoid double-recording: if this region overlaps any previously
        // recorded inline overlay (role or substitution), skip.
        let conflict = out.iter().any(|o| o.range.start < r.end && r.start < o.range.end);
        if conflict {
            i = j.saturating_add(1);
            continue;
        }
        out.push(InlineOverlayRegion {
            range: r,
            kind: InlineOverlayKind::PandocSpan {
                content_range: content_start..content_end,
                attrs_range: attrs_open.saturating_add(1)..attrs_close,
            },
        });
        i = attrs_close.saturating_add(1);
    }

    // Caller / overlay site expects regions in source order.
    out.sort_by_key(|r| r.range.start);
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
#[allow(
    clippy::indexing_slicing,
    reason = "test asserts; panic surface is the test framework"
)]
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
    fn frontmatter_opener_without_close_is_thematic_break() -> Result<()> {
        // `---\n` is a YAML opener, but with no closing `---` the
        // document is not a frontmatter — it's a thematic break
        // followed by Markdown. Confirming this via `prose_chunks`:
        // body text after the opener must surface as prose, not be
        // swallowed into a stub frontmatter.
        let src = "---\n\n- a\n- a\n\n- a\n";
        let ir = Ir::parse_str(src);
        assert!(ir.frontmatter.is_none(), "no frontmatter without close");
        let any_a = ir.prose_chunks.iter().any(|c| c.text == "a");
        assert!(
            any_a,
            "body markdown should be parsed as prose, got {:?}",
            ir.prose_chunks
        );
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
    fn abbreviation_scan_basic() -> Result<()> {
        let src = "Use HTML.\n\n*[HTML]: Hyper Text Markup Language\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.abbreviations.len(), 1);
        let r = ir.abbreviations.first().ok_or_else(|| anyhow!("region"))?;
        let bytes = src.get(r.range.clone()).unwrap_or("");
        assert_eq!(bytes, "*[HTML]: Hyper Text Markup Language\n");
        Ok(())
    }

    #[test]
    fn abbreviation_scan_multi_line() {
        let src = "*[A]: alpha\n*[B]: bravo\n*[C]: charlie\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.abbreviations.len(), 3);
    }

    #[test]
    fn abbreviation_scan_inside_code_block_skipped() {
        let src = "```\n*[HTML]: nope\n```\n";
        let ir = Ir::parse_str(src);
        assert!(ir.abbreviations.is_empty());
    }

    #[test]
    fn abbreviation_scan_inside_inline_code_skipped() {
        // A backtick run on the line means the entire line is inside
        // an inline code span; the abbreviation shape is not real.
        let src = "Some prose `*[X]: y` more.\n";
        let ir = Ir::parse_str(src);
        assert!(ir.abbreviations.is_empty());
    }

    #[test]
    fn abbreviation_scan_rejects_empty_term() {
        let src = "*[]: empty term\n";
        let ir = Ir::parse_str(src);
        assert!(ir.abbreviations.is_empty());
    }

    #[test]
    fn block_attr_scan_basic() -> Result<()> {
        let src = "Some prose.\n{ .note }\n";
        let ir = Ir::parse_str(src);
        assert_eq!(ir.block_attrs.len(), 1);
        let r = ir.block_attrs.first().ok_or_else(|| anyhow!("region"))?;
        // attrs_range covers just `{ .note }`.
        let attrs_bytes = src.get(r.attrs_range.clone()).unwrap_or("");
        assert_eq!(attrs_bytes, "{ .note }");
        Ok(())
    }

    #[test]
    fn block_attr_scan_requires_preceding_body() {
        // A standalone `{...}` line (no preceding body in the same
        // block) is not an attribute attachment.
        let src = "{ .note }\n";
        let ir = Ir::parse_str(src);
        assert!(ir.block_attrs.is_empty());
    }

    #[test]
    fn block_attr_scan_inside_code_block_skipped() {
        let src = "```\nbody\n{ .note }\n```\n";
        let ir = Ir::parse_str(src);
        assert!(ir.block_attrs.is_empty());
    }

    #[test]
    fn block_attr_scan_handles_blank_separator() {
        // mdformat-mkdocs only attaches a trailer when there is NO
        // blank line between the body and `{...}`. A blank line
        // makes them two separate blocks, so no attachment.
        let src = "Some prose.\n\n{ .note }\n";
        let ir = Ir::parse_str(src);
        assert!(ir.block_attrs.is_empty());
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

    use super::{DirectiveStyle, InlineOverlayKind, scan_comments, scan_directives, scan_inline_overlays};

    fn empty_ir_excludes() -> (
        Vec<super::CodeBlock>,
        Vec<super::InlineCode>,
        Vec<super::HtmlBlock>,
        Vec<super::InlineHtml>,
    ) {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
    }

    #[test]
    fn scan_directives_finds_myst_brace() {
        let src = ":::{note}\nbody\n:::\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_directives(src, &cb, &ic, &hb, &ih);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].style, DirectiveStyle::MystBrace);
        assert_eq!(&src[out[0].range.clone()], src);
    }

    #[test]
    fn scan_directives_finds_pandoc_attrs_and_short() {
        let attrs = "::: {.warning}\nbody\n:::\n";
        let short = ":::note\nbody\n:::\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        assert_eq!(
            scan_directives(attrs, &cb, &ic, &hb, &ih)[0].style,
            DirectiveStyle::PandocAttrs
        );
        assert_eq!(
            scan_directives(short, &cb, &ic, &hb, &ih)[0].style,
            DirectiveStyle::PandocShort
        );
    }

    #[test]
    fn scan_directives_allows_trailing_arg() {
        // `MyST` directives often carry an argument after the brace.
        let src = ":::{figure} ./img.png\n:alt: A diagram\n\nCaption.\n:::\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_directives(src, &cb, &ic, &hb, &ih);
        assert_eq!(out.len(), 1, "expected one region: {out:?}");
        assert_eq!(out[0].style, DirectiveStyle::MystBrace);
    }

    #[test]
    fn scan_directives_nested_records_outermost_only() {
        let src = "::::{note}\nouter\n\n:::{tip}\ninner\n:::\n::::\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_directives(src, &cb, &ic, &hb, &ih);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].colon_count, 4);
    }

    #[test]
    fn scan_directives_siblings_at_same_count() {
        let src = ":::{note}\nfirst\n:::\n\n:::{tip}\nsecond\n:::\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_directives(src, &cb, &ic, &hb, &ih);
        assert_eq!(out.len(), 2, "expected two sibling regions: {out:?}");
    }

    #[test]
    fn scan_directives_skips_unclosed_opener() {
        let src = ":::{note}\nbody never closes\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        assert!(scan_directives(src, &cb, &ic, &hb, &ih).is_empty());
    }

    #[test]
    fn scan_directives_empty_source() {
        let (cb, ic, hb, ih) = empty_ir_excludes();
        assert!(scan_directives("", &cb, &ic, &hb, &ih).is_empty());
        assert!(scan_directives("no directives here\n", &cb, &ic, &hb, &ih).is_empty());
    }

    #[test]
    fn scan_comments_recognises_line_start_percent() {
        let src = "para.\n\n% a comment\n\nafter.\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_comments(src, &cb, &ic, &hb, &ih);
        assert_eq!(out.len(), 1);
        assert_eq!(&src[out[0].range.clone()], "% a comment\n");
    }

    #[test]
    fn scan_comments_rejects_mid_paragraph_percent() {
        let src = "this is 50% complete and not a comment.\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        assert!(scan_comments(src, &cb, &ic, &hb, &ih).is_empty());
    }

    #[test]
    fn scan_comments_allows_leading_whitespace() {
        let src = "  % indented comment\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_comments(src, &cb, &ic, &hb, &ih);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn scan_inline_overlays_recognises_role() {
        let src = "see {term}`Vector Space` here\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_inline_overlays(src, &cb, &ic, &hb, &ih, &[], &[]);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0].kind, InlineOverlayKind::Role { .. }));
        assert_eq!(&src[out[0].range.clone()], "{term}`Vector Space`");
    }

    #[test]
    fn scan_inline_overlays_recognises_substitution() {
        let src = "use {{name}} here\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_inline_overlays(src, &cb, &ic, &hb, &ih, &[], &[]);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0].kind, InlineOverlayKind::Substitution { .. }));
    }

    #[test]
    fn scan_inline_overlays_recognises_pandoc_span() {
        let src = "a [bracketed bit]{.note} here\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_inline_overlays(src, &cb, &ic, &hb, &ih, &[], &[]);
        assert_eq!(out.len(), 1, "expected one span: {out:?}");
        assert!(matches!(out[0].kind, InlineOverlayKind::PandocSpan { .. }));
    }

    #[test]
    fn scan_inline_overlays_rejects_link_disguised_as_span() {
        // `[content](url)` is a CommonMark link, not a `Pandoc` span.
        let src = "a [link text](https://example.com) here\n";
        let (cb, ic, hb, ih) = empty_ir_excludes();
        let out = scan_inline_overlays(src, &cb, &ic, &hb, &ih, &[], &[]);
        assert!(out.is_empty(), "expected no overlays: {out:?}");
    }

    #[test]
    fn scan_inline_overlays_empty_source() {
        let (cb, ic, hb, ih) = empty_ir_excludes();
        assert!(scan_inline_overlays("", &cb, &ic, &hb, &ih, &[], &[]).is_empty());
        assert!(scan_inline_overlays("plain text\n", &cb, &ic, &hb, &ih, &[], &[]).is_empty());
    }
}
