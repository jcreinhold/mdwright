//! Braced super/subscript with a Unicode single-codepoint equivalent.
//!
//! `f^{-1}` reads more clearly as `f⁻¹` once the project commits to
//! Unicode mathematics. The rule recognises the closed set
//! `{^{-1}, ^{-d}, ^{0..9}, _{0..9}, ^{n,i}, _{n,i}}` and offers a
//! safe autofix. Advisory: informational, not a defect.

use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Fix};
use crate::regex_util::compile_static;
use crate::rule::LintRule;
use mdwright_document::Document;
use mdwright_latex::{unicode_sub, unicode_super};

pub struct UnicodeableSubscript;

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        compile_static(
            r"\^\{-1\}|\^\{-(?P<sneg>[0-9])\}|\^\{(?P<sd>[0-9])\}|_\{(?P<bd>[0-9])\}|\^\{n\}|_\{n\}|\^\{i\}|_\{i\}",
        )
    })
}

impl LintRule for UnicodeableSubscript {
    fn name(&self) -> &str {
        "unicodeable-subscript"
    }

    fn description(&self) -> &str {
        "Braced super/subscript that has a single-codepoint Unicode form."
    }

    fn explain(&self) -> &str {
        include_str!("explain/unicodeable_subscript.md")
    }

    fn produces_fix(&self) -> bool {
        true
    }

    fn is_advisory(&self) -> bool {
        true
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for chunk in doc.prose_chunks() {
            for cap in pattern().captures_iter(&chunk.text) {
                let Some(m) = cap.get(0) else { continue };
                let matched = m.as_str();
                let replacement = match matched {
                    "^{-1}" => "⁻¹".to_owned(),
                    "^{n}" => "ⁿ".to_owned(),
                    "_{n}" => "ₙ".to_owned(),
                    "^{i}" => "ⁱ".to_owned(),
                    "_{i}" => "ᵢ".to_owned(),
                    _ => {
                        if let Some(d) = cap.name("sneg") {
                            let Some(c) = d.as_str().chars().next() else {
                                continue;
                            };
                            match unicode_super(c) {
                                Some(u) => format!("⁻{u}"),
                                None => continue,
                            }
                        } else if let Some(d) = cap.name("sd") {
                            let Some(c) = d.as_str().chars().next() else {
                                continue;
                            };
                            match unicode_super(c) {
                                Some(u) => u.to_string(),
                                None => continue,
                            }
                        } else if let Some(d) = cap.name("bd") {
                            let Some(c) = d.as_str().chars().next() else {
                                continue;
                            };
                            match unicode_sub(c) {
                                Some(u) => u.to_string(),
                                None => continue,
                            }
                        } else {
                            continue;
                        }
                    }
                };
                let message = format!("`{matched}` has a Unicode equivalent `{replacement}` — clearer to read");
                if let Some(d) = Diagnostic::at(
                    doc,
                    chunk.byte_offset,
                    m.range(),
                    message,
                    Some(Fix {
                        replacement,
                        safe: true,
                    }),
                ) {
                    out.push(d);
                }
            }
        }
    }
}
