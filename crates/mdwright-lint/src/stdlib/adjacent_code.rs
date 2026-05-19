//! Inline code spans with no separating whitespace from a neighbouring
//! letter, e.g. `` `foo`bar ``.
//!
//! `CommonMark` renders these correctly, but several Markdown
//! renderers (mdformat with the mkdocs plugin, in particular)
//! re-tokenise ambiguously and surrounding `_` or `*` get mangled.
//! The structural fix is to always put a space between an inline
//! code span and an adjacent word.

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;
use mdwright_document::Document;

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

            let after_letter = bytes.get(end).copied().is_some_and(|b| b.is_ascii_alphabetic())
                && !is_plain_english_suffix(bytes, end);

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

fn is_plain_english_suffix(bytes: &[u8], end: usize) -> bool {
    match bytes.get(end..) {
        Some([b's', next, ..]) if !next.is_ascii_alphabetic() => true,
        Some([b'\'', b's', next, ..]) if !next.is_ascii_alphabetic() => true,
        Some([b's'] | [b'\'', b's']) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use mdwright_document::Document;

    use super::AdjacentCodeNoSpace;
    use crate::rule_set::RuleSet;

    fn diagnostics(src: &str) -> Result<usize> {
        let mut rules = RuleSet::new();
        rules
            .add(Box::new(AdjacentCodeNoSpace))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(rules.check(&Document::parse(src)?).len())
    }

    #[test]
    fn allows_common_inline_code_suffixes() -> Result<()> {
        assert_eq!(diagnostics("Use `TODO`s and `Vec`'s capacity.\n")?, 0);
        Ok(())
    }

    #[test]
    fn still_flags_word_glued_after_code_span() -> Result<()> {
        assert_eq!(diagnostics("Use `foo`bar here.\n")?, 1);
        Ok(())
    }
}
