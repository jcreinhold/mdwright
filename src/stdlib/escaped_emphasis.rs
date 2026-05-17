//! `\_` or `\*` literal escape in prose.
//!
//! Almost always damage from a Markdown formatter that escaped
//! italic delimiters after parsing a paragraph whose italic span
//! happened to contain a subscript `_`. Default-off because the
//! preferred fix is to use `*…*` italics from the start; the rule
//! exists to repair existing `mdformat`-mangled corpora.
//!
//! The new IR exposes raw prose chunks with the backslash intact —
//! something the prior implementation could not do because it sliced
//! pulldown-cmark's Text-event ranges, which omit preceding `\`.

use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Fix};
use crate::document::Document;
use crate::rule::LintRule;
use crate::util::regex::compile_static;

pub struct EscapedEmphasis;

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"\\[_*`]"))
}

impl LintRule for EscapedEmphasis {
    fn name(&self) -> &str {
        "escaped-emphasis"
    }

    fn description(&self) -> &str {
        "Literal `\\_`, `\\*`, or `` \\` `` escape in prose (mdformat damage)."
    }

    fn explain(&self) -> &str {
        include_str!("explain/escaped_emphasis.md")
    }

    fn produces_fix(&self) -> bool {
        true
    }

    fn is_default(&self) -> bool {
        false
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for chunk in doc.prose_chunks() {
            for m in pattern().find_iter(&chunk.text) {
                let target = m.as_str().as_bytes().get(1).copied();
                let (fix_char, label) = match target {
                    Some(b'_') => ("*", r"\_"),
                    Some(b'*') => ("*", r"\*"),
                    Some(b'`') => ("`", r"\`"),
                    _ => continue,
                };
                let message = format!(
                    "`{label}` escape — likely italic-delimiter damage from a previous \
                     formatter pass; prefer `*…*` for italics, never `_…_`"
                );
                if let Some(d) = Diagnostic::at(
                    doc,
                    chunk.byte_offset,
                    m.range(),
                    message,
                    Some(Fix {
                        replacement: fix_char.to_owned(),
                        safe: true,
                    }),
                ) {
                    out.push(d);
                }
            }
        }
    }
}
