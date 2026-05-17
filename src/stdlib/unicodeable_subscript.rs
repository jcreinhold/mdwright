//! Braced super/subscript with a Unicode single-codepoint equivalent.
//!
//! `f^{-1}` reads more clearly as `f⁻¹` once the project commits to
//! Unicode mathematics. The rule recognises the closed set
//! `{^{-1}, ^{-d}, ^{0..9}, _{0..9}, ^{n,i}, _{n,i}}` and offers a
//! safe autofix. Advisory: informational, not a defect.

use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Fix};
use crate::document::Document;
use crate::rule::LintRule;
use crate::util::regex::compile_static;

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
                let message = format!(
                    "`{matched}` has a Unicode equivalent `{replacement}` — clearer to read"
                );
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

fn unicode_super(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        '-' => '⁻',
        _ => return None,
    })
}

fn unicode_sub(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        'n' => 'ₙ',
        'i' => 'ᵢ',
        _ => return None,
    })
}
