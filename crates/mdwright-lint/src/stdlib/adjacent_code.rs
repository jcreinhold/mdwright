//! Inline code spans with no separating whitespace from a neighbouring
//! letter, e.g. `` `foo`bar ``.
//!
//! `CommonMark` renders these correctly, but several Markdown
//! renderers (mdformat with the mkdocs plugin, in particular)
//! re-tokenise ambiguously and surrounding `_` or `*` get mangled.
//! The structural fix is to always put a space between an inline
//! code span and an adjacent word.

use crate::diagnostic::{Diagnostic, Fix};
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

    fn produces_fix(&self) -> bool {
        true
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

            if before_letter {
                let message = "letter directly precedes an inline code span — insert a space \
                     between the word and the opening backtick"
                    .to_owned();
                let fix = Some(Fix {
                    replacement: " ".to_owned(),
                    safe: true,
                });
                if let Some(d) = Diagnostic::at(doc, start, 0..0, message, fix) {
                    out.push(d);
                }
            }

            if after_letter {
                let message = "letter directly follows an inline code span — insert a space \
                     between the closing backtick and the word"
                    .to_owned();
                let fix = Some(Fix {
                    replacement: " ".to_owned(),
                    safe: true,
                });
                if let Some(d) = Diagnostic::at(doc, end, 0..0, message, fix) {
                    out.push(d);
                }
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
    use crate::apply_safe_fixes;
    use crate::rule_set::RuleSet;

    fn rules() -> Result<RuleSet> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(AdjacentCodeNoSpace))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(rs)
    }

    fn diagnostic_count(src: &str) -> Result<usize> {
        Ok(rules()?.check(&Document::parse(src)?).len())
    }

    #[test]
    fn allows_common_inline_code_suffixes() -> Result<()> {
        assert_eq!(diagnostic_count("Use `TODO`s and `Vec`'s capacity.\n")?, 0);
        Ok(())
    }

    #[test]
    fn still_flags_word_glued_after_code_span() -> Result<()> {
        assert_eq!(diagnostic_count("Use `foo`bar here.\n")?, 1);
        Ok(())
    }

    #[test]
    fn flags_both_sides_when_glued_on_each_end() -> Result<()> {
        assert_eq!(diagnostic_count("Use x`foo`bar here.\n")?, 2);
        Ok(())
    }

    #[test]
    fn fix_inserts_space_after_code_span() -> Result<()> {
        let src = "Use `foo`bar here.\n";
        let doc = Document::parse(src)?;
        let diags = rules()?.check(&doc);
        let fix = diags
            .first()
            .and_then(|d| d.fix.as_ref())
            .ok_or_else(|| anyhow::anyhow!("fix"))?;
        assert!(fix.safe);
        assert_eq!(fix.replacement, " ");
        let (out, applied) = apply_safe_fixes(&doc, &diags);
        assert_eq!(applied, 1);
        assert_eq!(out, "Use `foo` bar here.\n");
        let doc2 = Document::parse(&out)?;
        assert!(rules()?.check(&doc2).is_empty());
        Ok(())
    }

    #[test]
    fn fix_inserts_space_on_both_sides() -> Result<()> {
        let src = "x`foo`bar\n";
        let doc = Document::parse(src)?;
        let diags = rules()?.check(&doc);
        assert_eq!(diags.len(), 2);
        let (out, applied) = apply_safe_fixes(&doc, &diags);
        assert_eq!(applied, 2);
        assert_eq!(out, "x `foo` bar\n");
        let doc2 = Document::parse(&out)?;
        assert!(rules()?.check(&doc2).is_empty());
        Ok(())
    }
}
