//! Inline code spans with no separating whitespace from a neighbouring
//! letter, e.g. `` `foo`bar ``.
//!
//! `CommonMark` renders these correctly, but several Markdown
//! renderers (mdformat with the mkdocs plugin, in particular)
//! re-tokenise ambiguously and surrounding `_` or `*` get mangled.
//! The structural fix is to always put a space between an inline
//! code span and an adjacent word.

use crate::diagnostic::Diagnostic;
use crate::document::Document;
use crate::rule::LintRule;

pub struct AdjacentCodeNoSpace;

impl LintRule for AdjacentCodeNoSpace {
    fn name(&self) -> &str {
        "adjacent-code-no-space"
    }

    fn description(&self) -> &str {
        "Inline code span adjacent to a letter without whitespace."
    }

    fn explain(&self) -> &str {
        include_str!("explain/adjacent_code_no_space.md")
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        let bytes = doc.source().as_bytes();
        for code in doc.inline_codes() {
            let start = code.raw_range.start;
            let end = code.raw_range.end;

            let before_letter = start
                .checked_sub(1)
                .and_then(|i| bytes.get(i).copied())
                .is_some_and(|b| b.is_ascii_alphabetic());

            let after_letter = bytes.get(end).copied().is_some_and(|b| b.is_ascii_alphabetic());

            if !before_letter && !after_letter {
                continue;
            }

            let message = "inline code adjacent to a letter without whitespace — insert a \
                 space between the code span and the surrounding word"
                .to_owned();

            if let Some(d) = Diagnostic::at(doc, start, 0..end.saturating_sub(start), message, None) {
                out.push(d);
            }
        }
    }
}
