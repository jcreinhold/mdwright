//! Bare `http(s)://…` in prose where an autolink would be cleaner.
//!
//! `CommonMark` autolinks (`<https://example.com>`) render as
//! clickable links across all renderers; bare URLs depend on
//! renderer-specific autolinking heuristics. The rule scans prose
//! chunks — autolinks already parsed by pulldown-cmark live inside
//! `Link` containers, so they don't appear in `prose_chunks` and
//! won't double-fire.

use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Fix};
use crate::document::Document;
use crate::rule::LintRule;
use crate::util::regex::compile_static;

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
                let message = format!("bare URL `{url}` — wrap as `<{url}>` for a portable autolink");
                let fix = Fix {
                    replacement: format!("<{url}>"),
                    safe: true,
                };
                if let Some(d) = Diagnostic::at(doc, chunk.byte_offset, m.start()..end, message, Some(fix)) {
                    out.push(d);
                }
            }
        }
    }
}
