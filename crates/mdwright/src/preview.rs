use anyhow::Result;
use mdwright_document::{Document, ExtensionOptions, ParseOptions};
use mdwright_latex::render_unicode_math;
use owo_colors::OwoColorize;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::html_highlight;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewMath {
    Unicode,
    Source,
    Off,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct PreviewOptions {
    pub(crate) color: bool,
    pub(crate) math: PreviewMath,
}

pub(crate) fn render_preview(doc: &Document, opts: PreviewOptions) -> Result<String> {
    let source = replace_math_regions(doc, opts.math);
    let mut renderer = PreviewRenderer::new(opts.color);
    renderer.render(&source, doc.parse_options())?;
    Ok(renderer.finish())
}

fn replace_math_regions(doc: &Document, mode: PreviewMath) -> String {
    if matches!(mode, PreviewMath::Off) || doc.math_regions().is_empty() {
        return doc.source().to_owned();
    }
    let source = doc.source();
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for region in doc.math_regions() {
        if cursor < region.range.start
            && let Some(prefix) = source.get(cursor..region.range.start)
        {
            out.push_str(prefix);
        }
        let original = source.get(region.range.clone()).unwrap_or("");
        match mode {
            PreviewMath::Source => out.push_str(&escape_backslashes_for_markdown(original)),
            PreviewMath::Off => out.push_str(original),
            PreviewMath::Unicode => {
                let body = region.span().body().as_str(source);
                match render_unicode_math(body.as_ref()) {
                    Ok(rendered) => out.push_str(&rendered.as_text()),
                    Err(_) => out.push_str(original),
                }
            }
        }
        cursor = region.range.end;
    }
    if cursor < source.len()
        && let Some(suffix) = source.get(cursor..)
    {
        out.push_str(suffix);
    }
    out
}

fn escape_backslashes_for_markdown(source: &str) -> String {
    source.replace('\\', r"\\")
}

struct PreviewRenderer {
    out: String,
    color: bool,
    list_stack: Vec<ListState>,
    pending_item: bool,
    in_code_block: Option<String>,
    table_cell_open: bool,
}

#[derive(Copy, Clone, Debug)]
struct ListState {
    ordered: bool,
    next: u64,
}

impl PreviewRenderer {
    fn new(color: bool) -> Self {
        Self {
            out: String::new(),
            color,
            list_stack: Vec::new(),
            pending_item: false,
            in_code_block: None,
            table_cell_open: false,
        }
    }

    fn render(&mut self, source: &str, parse_options: ParseOptions) -> Result<()> {
        let parser = Parser::new_ext(source, pulldown_options(parse_options));
        for event in parser {
            self.event(event)?;
        }
        Ok(())
    }

    fn finish(mut self) -> String {
        while self.out.ends_with("\n\n\n") {
            self.out.pop();
        }
        self.out
    }

    fn event(&mut self, event: Event<'_>) -> Result<()> {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if let Some(lang) = self.in_code_block.clone() {
                    self.write_code(&text, &lang);
                } else {
                    self.write_inline(&text);
                }
            }
            Event::Code(code) => self.write_inline(&format!("`{code}`")),
            Event::InlineHtml(html) | Event::Html(html) => self.write_inline(&html),
            Event::SoftBreak | Event::HardBreak => self.newline(),
            Event::Rule => {
                self.ensure_block_start();
                self.out.push_str("────────\n\n");
            }
            Event::TaskListMarker(checked) => {
                self.write_inline(if checked { "[x] " } else { "[ ] " });
            }
            Event::FootnoteReference(label) => self.write_inline(&format!("[^{label}]")),
            Event::InlineMath(math) | Event::DisplayMath(math) => self.write_inline(&math),
        }
        Ok(())
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.ensure_block_start();
                let marker = "#".repeat(heading_depth(level));
                self.out.push_str(&style(&marker, self.color, Style::Muted));
                self.out.push(' ');
            }
            Tag::BlockQuote(_) => {
                self.ensure_block_start();
                self.out.push_str(&style("│ ", self.color, Style::Muted));
            }
            Tag::CodeBlock(kind) => {
                self.ensure_block_start();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_owned(),
                    CodeBlockKind::Indented => String::new(),
                };
                if !lang.is_empty() {
                    self.out
                        .push_str(&style(&format!("┌ {lang}\n"), self.color, Style::Muted));
                }
                self.in_code_block = Some(lang);
            }
            Tag::List(start) => {
                self.list_stack.push(ListState {
                    ordered: start.is_some(),
                    next: start.unwrap_or(1),
                });
            }
            Tag::Item => {
                self.ensure_line_start();
                let indent = self.list_stack.len().saturating_sub(1).saturating_mul(2);
                self.out.push_str(&" ".repeat(indent));
                if let Some(state) = self.list_stack.last_mut() {
                    if state.ordered {
                        self.out.push_str(&state.next.to_string());
                        self.out.push_str(". ");
                        state.next = state.next.saturating_add(1);
                    } else {
                        self.out.push_str("• ");
                    }
                }
                self.pending_item = true;
            }
            Tag::Emphasis | Tag::Strong | Tag::Strikethrough => {}
            Tag::Link { dest_url, .. } => {
                if !dest_url.is_empty() {
                    self.write_inline(&style("↗ ", self.color, Style::Link));
                }
            }
            Tag::Image { .. } => self.write_inline("[image: "),
            Tag::Table(_) => self.ensure_block_start(),
            Tag::TableHead | Tag::TableRow => {
                self.ensure_line_start();
                self.out.push('|');
            }
            Tag::TableCell => {
                self.table_cell_open = true;
                self.out.push(' ');
            }
            Tag::FootnoteDefinition(label) => {
                self.ensure_block_start();
                self.write_inline(&format!("[^{label}]: "));
            }
            Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {}
            Tag::HtmlBlock | Tag::MetadataBlock(_) | Tag::Superscript | Tag::Subscript => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                if self.pending_item {
                    self.pending_item = false;
                    self.newline();
                } else {
                    self.blank_line();
                }
            }
            TagEnd::Heading(_) => self.blank_line(),
            TagEnd::BlockQuote(_) => self.blank_line(),
            TagEnd::CodeBlock => {
                self.in_code_block = None;
                self.out.push('\n');
                self.out.push_str(&style("└\n\n", self.color, Style::Muted));
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
                self.blank_line();
            }
            TagEnd::Item => {
                self.pending_item = false;
                self.newline();
            }
            TagEnd::TableCell => {
                if self.table_cell_open {
                    self.out.push(' ');
                    self.out.push('|');
                    self.table_cell_open = false;
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => self.newline(),
            TagEnd::Table => self.blank_line(),
            TagEnd::FootnoteDefinition => self.blank_line(),
            TagEnd::Image => self.write_inline("]"),
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::HtmlBlock
            | TagEnd::MetadataBlock(_)
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition => {}
        }
    }

    fn write_code(&mut self, text: &str, lang: &str) {
        if self.color
            && !lang.is_empty()
            && let Some(highlighted) = html_highlight::highlight_code(text, lang)
        {
            self.out.push_str(&highlighted);
            return;
        }
        self.out.push_str(text);
    }

    fn write_inline(&mut self, text: &str) {
        self.out.push_str(text);
    }

    fn newline(&mut self) {
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn blank_line(&mut self) {
        self.newline();
        if !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    fn ensure_line_start(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn ensure_block_start(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.blank_line();
        }
    }
}

#[derive(Copy, Clone)]
enum Style {
    Link,
    Muted,
}

fn style(text: &str, color: bool, style: Style) -> String {
    if !color {
        return text.to_owned();
    }
    match style {
        Style::Link => text.blue().to_string(),
        Style::Muted => text.bright_black().to_string(),
    }
}

fn heading_depth(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn pulldown_options(parse_options: ParseOptions) -> Options {
    let mut opts = Options::empty();
    let ext: ExtensionOptions = parse_options.extensions();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_DEFINITION_LIST);
    opts.insert(Options::ENABLE_GFM);
    if ext.heading_attribute_lists {
        opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    }
    opts
}

#[cfg(test)]
fn render_source(source: &str, parse_options: ParseOptions, opts: PreviewOptions) -> Result<String> {
    use anyhow::Context as _;

    let doc = Document::parse_with_options(source, parse_options).context("parse preview input")?;
    render_preview(&doc, opts)
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "unit tests use expect to surface setup failures")]
mod tests {
    use super::*;

    #[test]
    fn unicode_math_preview_uses_first_party_renderer() {
        let out = render_source(
            r"# Math

\[ \frac{\alpha_i}{x^{2}} \]
",
            ParseOptions::default(),
            PreviewOptions {
                color: false,
                math: PreviewMath::Unicode,
            },
        )
        .expect("preview renders");
        assert!(out.contains("αᵢ"), "{out}");
        assert!(out.contains("──"), "{out}");
        assert!(!out.contains("<h1>"), "{out}");
    }

    #[test]
    fn source_math_preview_preserves_source_math() {
        let out = render_source(
            r"\[ \alpha_i \]",
            ParseOptions::default(),
            PreviewOptions {
                color: false,
                math: PreviewMath::Source,
            },
        )
        .expect("preview renders");
        assert!(out.contains(r"\[ \alpha_i \]"), "{out}");
    }
}
