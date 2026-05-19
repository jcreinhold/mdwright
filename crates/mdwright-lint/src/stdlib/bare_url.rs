//! Bare `http(s)://…` in prose where an autolink would be cleaner.
//!
//! `CommonMark` autolinks (`<https://example.com>`) render as
//! clickable links across all renderers; bare URLs depend on
//! renderer-specific autolinking heuristics. The rule scans prose
//! chunks and GFM bare-autolink facts. Explicit `CommonMark` autolinks
//! (`<https://example.com>`) and Markdown links are already portable,
//! so their ranges are excluded from the prose scan.

use std::ops::Range;
use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Fix};
use crate::regex_util::compile_static;
use crate::rule::LintRule;
use mdwright_document::{Document, NodeKind};

pub struct BareUrl;

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r#"https?://[^\s<>()\[\]`'"]+"#))
}

impl LintRule for BareUrl {
    fn name(&self) -> &str {
        "bare-url"
    }

    fn description(&self) -> &str {
        "Bare URL in prose; wrap in `<…>` for a CommonMark autolink."
    }

    fn explain(&self) -> &str {
        include_str!("explain/bare_url.md")
    }

    fn produces_fix(&self) -> bool {
        true
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        let excluded = link_like_ranges(doc);
        for autolink in doc.gfm_bare_autolinks() {
            if should_flag_bare_url(&autolink.text) {
                push_diagnostic(doc, autolink.raw_range.clone(), &autolink.text, out);
            }
        }
        for chunk in doc.prose_chunks() {
            for m in pattern().find_iter(&chunk.text) {
                let mut end = m.end();
                while end > m.start() {
                    let last = chunk.text.as_bytes().get(end.saturating_sub(1)).copied();
                    if matches!(last, Some(b'.' | b',' | b';' | b':' | b'!' | b'?')) {
                        end = end.saturating_sub(1);
                    } else {
                        break;
                    }
                }
                let url = chunk.text.get(m.start()..end).unwrap_or("");
                if url.is_empty() {
                    continue;
                }
                let raw_range = chunk.byte_offset.saturating_add(m.start())..chunk.byte_offset.saturating_add(end);
                if !ranges_overlap_any(&raw_range, &excluded) {
                    push_diagnostic(doc, raw_range, url, out);
                }
            }
        }
    }
}

fn should_flag_bare_url(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

fn push_diagnostic(doc: &Document, raw_range: Range<usize>, url: &str, out: &mut Vec<Diagnostic>) {
    let message = format!("bare URL `{url}` — wrap as `<{url}>` for a portable autolink");
    let fix = Fix {
        replacement: format!("<{url}>"),
        safe: true,
    };
    let local = 0..raw_range.end.saturating_sub(raw_range.start);
    if let Some(d) = Diagnostic::at(doc, raw_range.start, local, message, Some(fix)) {
        out.push(d);
    }
}

fn link_like_ranges(doc: &Document) -> Vec<Range<usize>> {
    let tree = doc.tree();
    let mut ranges = Vec::new();
    for id in tree.descendants(tree.root()) {
        let Some(node) = tree.node(id) else { continue };
        if matches!(
            node.kind,
            NodeKind::Link { .. } | NodeKind::Image { .. } | NodeKind::Autolink
        ) {
            ranges.push(node.raw_range.clone());
        }
    }
    ranges.extend(
        doc.gfm_bare_autolinks()
            .iter()
            .map(|autolink| autolink.raw_range.clone()),
    );
    ranges
}

fn ranges_overlap_any(range: &Range<usize>, others: &[Range<usize>]) -> bool {
    others
        .iter()
        .any(|other| range.start < other.end && other.start < range.end)
}
