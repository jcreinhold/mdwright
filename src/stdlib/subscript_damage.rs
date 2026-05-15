//! `Hom*{…}`, `α*f`, etc.: math identifier whose subscript `_` was
//! rewritten to `*` emphasis by an earlier formatter pass.
//!
//! Scans prose chunks and inline code (the broken text sometimes
//! lives verbatim inside a code span if the original `_` was
//! preceded by a backtick). The allowlist guards against the common
//! legitimate pullback notation `f*`, `λ*`, etc.: bare one-character
//! identifiers that conventionally apply pullback.
//!
//! Default-off — repair-focused, useful for cleaning legacy
//! `mdformat`-mangled documents.

use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Fix};
use crate::document::Document;
use crate::rule::LintRule;
use crate::util::regex::compile_static;

pub struct SubscriptDamage;

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"(?P<head>[A-Za-z\p{Greek}\p{Letter}])\*(?P<tail>[A-Za-z\{])"))
}

const PULLBACK_HEADS: &[&str] = &["λ", "f", "F", "g", "G", "u", "v", "h"];

impl LintRule for SubscriptDamage {
    fn name(&self) -> &str {
        "subscript-damage"
    }

    fn description(&self) -> &str {
        "Identifier with `*` where a `_` subscript was expected (formatter damage)."
    }

    fn is_default(&self) -> bool {
        false
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        for chunk in doc.prose_chunks() {
            scan(chunk.text, chunk.byte_offset, doc, out);
        }
        for code in doc.inline_codes() {
            scan(code.text, code.byte_offset, doc, out);
        }
    }
}

fn scan(text: &str, offset: usize, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
    for cap in pattern().captures_iter(text) {
        let Some(m) = cap.get(0) else { continue };
        let head = cap.name("head").map_or("", |x| x.as_str());
        if PULLBACK_HEADS.contains(&head) {
            continue;
        }
        let matched = m.as_str();
        let fixed: String = matched.chars().map(|c| if c == '*' { '_' } else { c }).collect();
        let message = format!(
            "`{matched}` looks like subscript damage (`_` rewritten to `*` after a \
             broken code span); intended math is likely `{fixed}`"
        );
        if let Some(d) = Diagnostic::at(
            doc,
            offset,
            m.range(),
            message,
            Some(Fix {
                replacement: fixed,
                safe: false,
            }),
        ) {
            out.push(d);
        }
    }
}
