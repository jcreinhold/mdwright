//! HTML rendering policy owned by the document crate.
//!
//! Production parsing still flows through `crate::parse`; this module
//! only decides how a recognised event stream is spelled as HTML.

use std::collections::HashMap;
use std::fmt::Write as _;

use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, Tag, TagEnd};

/// HTML spelling policy for [`crate::render_html_with_render_options`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum RenderProfile {
    /// Use pulldown-cmark's built-in HTML renderer.
    #[default]
    Pulldown,
    /// Spell HTML like cmark-gfm where that can be done without
    /// changing parser semantics.
    CmarkGfm,
}

/// Rendering policy for source-to-HTML helpers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct RenderOptions {
    profile: RenderProfile,
}

impl RenderOptions {
    /// HTML spelling profile.
    #[must_use]
    pub fn profile(&self) -> RenderProfile {
        self.profile
    }

    /// Override the HTML spelling profile.
    #[must_use]
    pub fn with_profile(mut self, profile: RenderProfile) -> Self {
        self.profile = profile;
        self
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TableState {
    Head,
    Body,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TableBodyState {
    Closed,
    Open,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TableCellState {
    Closed,
    Open,
}

pub(crate) fn render_cmark_gfm_html(events: Vec<Event<'_>>) -> String {
    CmarkGfmHtml::new(events.into_iter().map(Event::into_static)).run()
}

struct CmarkGfmHtml<I> {
    iter: I,
    out: String,
    end_newline: bool,
    metadata_depth: u32,
    html_block_depth: u32,
    table_state: TableState,
    table_alignments: Vec<Alignment>,
    table_cell_index: usize,
    table_body: TableBodyState,
    table_cell: TableCellState,
    numbers: HashMap<String, usize>,
}

impl<I> CmarkGfmHtml<I>
where
    I: Iterator<Item = Event<'static>>,
{
    fn new(iter: I) -> Self {
        Self {
            iter,
            out: String::new(),
            end_newline: true,
            metadata_depth: 0,
            html_block_depth: 0,
            table_state: TableState::Head,
            table_alignments: Vec::new(),
            table_cell_index: 0,
            table_body: TableBodyState::Closed,
            table_cell: TableCellState::Closed,
            numbers: HashMap::new(),
        }
    }

    fn run(mut self) -> String {
        while let Some(event) = self.iter.next() {
            match event {
                Event::Start(tag) => self.start_tag(tag),
                Event::End(tag) => self.end_tag(tag),
                Event::Text(text) => {
                    if self.metadata_depth == 0 {
                        self.escape_body_text(text.as_ref());
                    }
                }
                Event::Code(text) => {
                    self.write("<code>");
                    self.escape_body_text(text.as_ref());
                    self.write("</code>");
                }
                Event::InlineMath(text) => {
                    self.write(r#"<span class="math math-inline">"#);
                    self.escape_attr(text.as_ref());
                    self.write("</span>");
                }
                Event::DisplayMath(text) => {
                    self.write(r#"<span class="math math-display">"#);
                    self.escape_attr(text.as_ref());
                    self.write("</span>");
                }
                Event::Html(html) => {
                    if self.html_block_depth > 0 && !self.end_newline {
                        self.write_newline();
                    }
                    self.write(html.as_ref());
                }
                Event::InlineHtml(html) => self.write(html.as_ref()),
                Event::FootnoteReference(name) => self.footnote_reference(name),
                Event::SoftBreak => self.write_newline(),
                Event::HardBreak => self.write("<br />\n"),
                Event::Rule => {
                    if !self.end_newline {
                        self.write_newline();
                    }
                    self.write("<hr />\n");
                }
                Event::TaskListMarker(false) => self.write(r#"<input type="checkbox" disabled="" /> "#),
                Event::TaskListMarker(true) => self.write(r#"<input type="checkbox" checked="" disabled="" /> "#),
            }
        }
        self.out
    }

    fn write(&mut self, s: &str) {
        self.out.push_str(s);
        if !s.is_empty() {
            self.end_newline = s.ends_with('\n');
        }
    }

    fn write_newline(&mut self) {
        self.out.push('\n');
        self.end_newline = true;
    }

    fn start_tag(&mut self, tag: Tag<'static>) {
        match tag {
            Tag::HtmlBlock => {
                self.html_block_depth = self.html_block_depth.saturating_add(1);
            }
            Tag::Paragraph => self.open_block("<p>"),
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => self.heading_start(level, id, &classes, &attrs),
            Tag::BlockQuote(kind) => self.blockquote_start(kind),
            Tag::CodeBlock(kind) => self.code_block_start(kind),
            Tag::List(Some(1)) => self.open_line("<ol>\n"),
            Tag::List(Some(start)) => {
                if !self.end_newline {
                    self.write_newline();
                }
                self.write(r#"<ol start=""#);
                let _ = write!(self.out, "{start}");
                self.write("\">\n");
            }
            Tag::List(None) => self.open_line("<ul>\n"),
            Tag::Item => self.open_block("<li>"),
            Tag::DefinitionList => self.open_line("<dl>\n"),
            Tag::DefinitionListTitle => self.open_block("<dt>"),
            Tag::DefinitionListDefinition => self.open_block("<dd>"),
            Tag::Table(alignments) => {
                self.table_alignments = alignments;
                self.table_state = TableState::Head;
                self.table_body = TableBodyState::Closed;
                self.write("<table>\n");
            }
            Tag::TableHead => {
                self.table_state = TableState::Head;
                self.table_cell_index = 0;
                self.write("<thead>\n<tr>\n");
            }
            Tag::TableRow => {
                if self.table_state == TableState::Body && self.table_body == TableBodyState::Closed {
                    self.table_body = TableBodyState::Open;
                    self.write("<tbody>\n");
                }
                self.table_cell_index = 0;
                self.write("<tr>\n");
            }
            Tag::TableCell => self.table_cell_start(),
            Tag::Subscript => self.write("<sub>"),
            Tag::Superscript => self.write("<sup>"),
            Tag::Emphasis => self.write("<em>"),
            Tag::Strong => self.write("<strong>"),
            Tag::Strikethrough => self.write("<del>"),
            Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            } => self.link_start(link_type, &dest_url, &title),
            Tag::Image { dest_url, title, .. } => self.image(&dest_url, &title),
            Tag::FootnoteDefinition(name) => self.footnote_definition_start(name),
            Tag::MetadataBlock(_) => self.metadata_depth = self.metadata_depth.saturating_add(1),
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::HtmlBlock => self.html_block_depth = self.html_block_depth.saturating_sub(1),
            TagEnd::Paragraph => self.write("</p>\n"),
            TagEnd::Heading(level) => {
                self.write("</");
                self.write(&level.to_string());
                self.write(">\n");
            }
            TagEnd::BlockQuote(_) => self.write("</blockquote>\n"),
            TagEnd::CodeBlock => self.write("</code></pre>\n"),
            TagEnd::List(true) => self.write("</ol>\n"),
            TagEnd::List(false) => self.write("</ul>\n"),
            TagEnd::Item => self.write("</li>\n"),
            TagEnd::DefinitionList => self.write("</dl>\n"),
            TagEnd::DefinitionListTitle => self.write("</dt>\n"),
            TagEnd::DefinitionListDefinition => self.write("</dd>\n"),
            TagEnd::Table => {
                if self.table_body == TableBodyState::Open {
                    self.write("</tbody>\n");
                }
                self.write("</table>\n");
            }
            TagEnd::TableHead => {
                self.write("</tr>\n</thead>\n");
                self.table_state = TableState::Body;
            }
            TagEnd::TableRow => {
                self.close_table_cell_if_open();
                self.write("</tr>\n");
            }
            TagEnd::TableCell => {
                self.close_table_cell_if_open();
            }
            TagEnd::Emphasis => self.write("</em>"),
            TagEnd::Superscript => self.write("</sup>"),
            TagEnd::Subscript => self.write("</sub>"),
            TagEnd::Strong => self.write("</strong>"),
            TagEnd::Strikethrough => self.write("</del>"),
            TagEnd::Link => self.write("</a>"),
            TagEnd::Image => {}
            TagEnd::FootnoteDefinition => self.write("</div>\n"),
            TagEnd::MetadataBlock(_) => self.metadata_depth = self.metadata_depth.saturating_sub(1),
        }
    }

    fn open_block(&mut self, tag: &str) {
        if !self.end_newline {
            self.write_newline();
        }
        self.write(tag);
    }

    fn open_line(&mut self, tag: &str) {
        if !self.end_newline {
            self.write_newline();
        }
        self.write(tag);
    }

    fn heading_start(
        &mut self,
        level: HeadingLevel,
        id: Option<CowStr<'static>>,
        classes: &[CowStr<'static>],
        attrs: &[(CowStr<'static>, Option<CowStr<'static>>)],
    ) {
        if !self.end_newline {
            self.write_newline();
        }
        self.write("<");
        self.write(&level.to_string());
        if let Some(id) = id {
            self.write(r#" id=""#);
            self.escape_attr(id.as_ref());
            self.write("\"");
        }
        if !classes.is_empty() {
            self.write(r#" class=""#);
            for (i, class) in classes.iter().enumerate() {
                if i > 0 {
                    self.write(" ");
                }
                self.escape_attr(class.as_ref());
            }
            self.write("\"");
        }
        for (attr, value) in attrs {
            self.write(" ");
            self.escape_attr(attr.as_ref());
            self.write(r#"=""#);
            if let Some(value) = value {
                self.escape_attr(value.as_ref());
            }
            self.write("\"");
        }
        self.write(">");
    }

    fn blockquote_start(&mut self, kind: Option<BlockQuoteKind>) {
        if !self.end_newline {
            self.write_newline();
        }
        match kind {
            None => self.write("<blockquote>\n"),
            Some(BlockQuoteKind::Note) => self.write(r#"<blockquote class="markdown-alert-note">"#),
            Some(BlockQuoteKind::Tip) => self.write(r#"<blockquote class="markdown-alert-tip">"#),
            Some(BlockQuoteKind::Important) => self.write(r#"<blockquote class="markdown-alert-important">"#),
            Some(BlockQuoteKind::Warning) => self.write(r#"<blockquote class="markdown-alert-warning">"#),
            Some(BlockQuoteKind::Caution) => self.write(r#"<blockquote class="markdown-alert-caution">"#),
        }
        if kind.is_some() {
            self.write_newline();
        }
    }

    fn code_block_start(&mut self, kind: CodeBlockKind<'static>) {
        if !self.end_newline {
            self.write_newline();
        }
        match kind {
            CodeBlockKind::Indented => self.write("<pre><code>"),
            CodeBlockKind::Fenced(info) => {
                let lang = info.split(' ').next().unwrap_or_default();
                if lang.is_empty() {
                    self.write("<pre><code>");
                } else {
                    self.write(r#"<pre><code class="language-"#);
                    self.escape_attr(lang);
                    self.write("\">");
                }
            }
        }
    }

    fn table_cell_start(&mut self) {
        self.close_table_cell_if_open();
        let (tag, end) = match self.table_state {
            TableState::Head => ("<th", ">"),
            TableState::Body => ("<td", ">"),
        };
        self.write(tag);
        match self.table_alignments.get(self.table_cell_index) {
            Some(Alignment::Left) => self.write(r#" align="left""#),
            Some(Alignment::Center) => self.write(r#" align="center""#),
            Some(Alignment::Right) => self.write(r#" align="right""#),
            Some(Alignment::None) | None => {}
        }
        self.write(end);
        self.table_cell = TableCellState::Open;
    }

    fn close_table_cell_if_open(&mut self) {
        if self.table_cell == TableCellState::Closed {
            return;
        }
        match self.table_state {
            TableState::Head => self.write("</th>\n"),
            TableState::Body => self.write("</td>\n"),
        }
        self.table_cell_index = self.table_cell_index.saturating_add(1);
        self.table_cell = TableCellState::Closed;
    }

    fn link_start(&mut self, link_type: LinkType, dest_url: &CowStr<'static>, title: &CowStr<'static>) {
        self.write(r#"<a href=""#);
        if link_type == LinkType::Email && !dest_url.starts_with("mailto:") {
            self.write("mailto:");
        }
        self.escape_href(dest_url.as_ref());
        if !title.is_empty() {
            self.write(r#"" title=""#);
            self.escape_attr(title.as_ref());
        }
        self.write("\">");
    }

    fn image(&mut self, dest_url: &CowStr<'static>, title: &CowStr<'static>) {
        self.write(r#"<img src=""#);
        self.escape_href(dest_url.as_ref());
        self.write(r#"" alt=""#);
        self.raw_text();
        if !title.is_empty() {
            self.write(r#"" title=""#);
            self.escape_attr(title.as_ref());
        }
        self.write(r#"" />"#);
    }

    fn footnote_reference(&mut self, name: CowStr<'static>) {
        let len = self.numbers.len().saturating_add(1);
        self.write(r##"<sup class="footnote-reference"><a href="#"##);
        self.escape_attr(name.as_ref());
        self.write("\">");
        let number = *self.numbers.entry(name.into_string()).or_insert(len);
        let _ = write!(self.out, "{number}");
        self.write("</a></sup>");
    }

    fn footnote_definition_start(&mut self, name: CowStr<'static>) {
        if !self.end_newline {
            self.write_newline();
        }
        self.write(r#"<div class="footnote-definition" id=""#);
        self.escape_attr(name.as_ref());
        self.write(r#""><sup class="footnote-definition-label">"#);
        let len = self.numbers.len().saturating_add(1);
        let number = *self.numbers.entry(name.into_string()).or_insert(len);
        let _ = write!(self.out, "{number}");
        self.write("</sup>");
    }

    fn raw_text(&mut self) {
        let mut nest = 0usize;
        while let Some(event) = self.iter.next() {
            match event {
                Event::Start(_) => nest = nest.saturating_add(1),
                Event::End(_) if nest == 0 => break,
                Event::End(_) => nest = nest.saturating_sub(1),
                Event::Html(_) => {}
                Event::InlineHtml(text) | Event::Code(text) | Event::Text(text) => self.escape_attr(text.as_ref()),
                Event::InlineMath(text) => {
                    self.write("$");
                    self.escape_attr(text.as_ref());
                    self.write("$");
                }
                Event::DisplayMath(text) => {
                    self.write("$$");
                    self.escape_attr(text.as_ref());
                    self.write("$$");
                }
                Event::SoftBreak | Event::HardBreak | Event::Rule => self.write(" "),
                Event::FootnoteReference(name) => {
                    let len = self.numbers.len().saturating_add(1);
                    let number = *self.numbers.entry(name.into_string()).or_insert(len);
                    let _ = write!(self.out, "[{number}]");
                }
                Event::TaskListMarker(true) => self.write("[x]"),
                Event::TaskListMarker(false) => self.write("[ ]"),
            }
        }
    }

    fn escape_body_text(&mut self, s: &str) {
        escape_html(&mut self.out, s);
        self.end_newline = s.ends_with('\n');
    }

    fn escape_attr(&mut self, s: &str) {
        escape_html(&mut self.out, s);
        self.end_newline = s.ends_with('\n');
    }

    fn escape_href(&mut self, s: &str) {
        escape_href(&mut self.out, s);
        self.end_newline = s.ends_with('\n');
    }
}

fn escape_html(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn escape_href(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#x27;"),
            '"' | '<' | '>' | '\\' | '[' | ']' | '`' => {
                let mut buf = [0u8; 4];
                for byte in ch.encode_utf8(&mut buf).as_bytes() {
                    push_percent_byte(out, *byte);
                }
            }
            ch if ch.is_ascii_control() || ch == ' ' || !ch.is_ascii() => {
                let mut buf = [0u8; 4];
                for byte in ch.encode_utf8(&mut buf).as_bytes() {
                    push_percent_byte(out, *byte);
                }
            }
            _ => out.push(ch),
        }
    }
}

fn push_percent_byte(out: &mut String, byte: u8) {
    out.push('%');
    out.push(hex_digit(byte >> 4));
    out.push(hex_digit(byte & 0x0f));
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'A',
        11 => 'B',
        12 => 'C',
        13 => 'D',
        14 => 'E',
        15 => 'F',
        _ => '0',
    }
}
