//! Trailing `.` or `:` on an ATX or setext heading.
//!
//! Headings are titles, not sentences. A trailing period reads as
//! a stray dot; a trailing colon usually means the author wanted a
//! list or paragraph instead.

use crate::diagnostic::{Diagnostic, Fix};
use crate::rule::LintRule;
use mdwright_document::Document;

pub struct HeadingPunctuation;

impl LintRule for HeadingPunctuation {
    fn name(&self) -> &str {
        "heading-punctuation"
    }

    fn description(&self) -> &str {
        "Trailing `.` or `:` on a heading."
    }

    fn explain(&self) -> &str {
        include_str!("explain/heading_punctuation.md")
    }

    fn produces_fix(&self) -> bool {
        true
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for h in doc.headings() {
            let last = h.text.chars().last();
            let Some(c) = last else { continue };
            if c != '.' && c != ':' {
                continue;
            }
            let trailing_byte_len = c.len_utf8();
            let local_start = h.text.len().saturating_sub(trailing_byte_len);
            let local_end = h.text.len();
            let message = format!("heading ends with `{c}` — headings should not carry sentence punctuation");
            let fix = Some(Fix {
                replacement: String::new(),
                safe: true,
            });
            if let Some(d) = Diagnostic::at(doc, h.byte_offset, local_start..local_end, message, fix) {
                out.push(d);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use mdwright_document::Document;

    use super::HeadingPunctuation;
    use crate::apply_safe_fixes;
    use crate::rule_set::RuleSet;

    fn rules() -> Result<RuleSet> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(HeadingPunctuation))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(rs)
    }

    #[test]
    fn emits_safe_fix() -> Result<()> {
        let doc = Document::parse("# Title.\n")?;
        let diags = rules()?.check(&doc);
        assert_eq!(diags.len(), 1);
        let fix = diags
            .first()
            .and_then(|d| d.fix.as_ref())
            .ok_or_else(|| anyhow::anyhow!("fix"))?;
        assert!(fix.safe);
        assert_eq!(fix.replacement, "");
        Ok(())
    }

    #[test]
    fn fix_strips_trailing_punctuation_and_is_idempotent() -> Result<()> {
        let src = "# Intro:\n\n## Methods.\n";
        let doc = Document::parse(src)?;
        let diags = rules()?.check(&doc);
        let (out, applied) = apply_safe_fixes(&doc, &diags);
        assert_eq!(applied, 2);
        assert_eq!(out, "# Intro\n\n## Methods\n");
        let doc2 = Document::parse(&out)?;
        assert!(rules()?.check(&doc2).is_empty());
        Ok(())
    }
}
