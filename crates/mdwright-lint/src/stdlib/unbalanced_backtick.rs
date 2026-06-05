//! A backtick in prose that pulldown-cmark could not pair.
//!
//! `CommonMark`'s parser pairs backtick runs greedily. If a literal
//! `` ` `` survives into a prose chunk, no matching closing fence
//! was found and the inline code span did not close. Renderers that
//! treat the unmatched run as prose tend to mangle nearby `_` or `*`
//! — flagging this early prevents that.

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;
use mdwright_document::Document;

pub struct UnbalancedBacktick;

impl LintRule for UnbalancedBacktick {
    fn name(&self) -> &str {
        "unbalanced-backtick"
    }

    fn description(&self) -> &str {
        "Backtick in prose that could not be paired with a closing fence."
    }

    fn explain(&self) -> &str {
        include_str!("explain/unbalanced_backtick.md")
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        let source = doc.source().as_bytes();
        for chunk in doc.prose_chunks() {
            for (idx, _) in chunk.text.match_indices('`') {
                // A `\`` is a CommonMark-escaped literal backtick, not an
                // unpaired code-span delimiter. Parity is computed against
                // the full source at the absolute offset, not within
                // `chunk.text`: prose chunks can split mid-backslash-run
                // (a `\\` escape straddling two chunks), so a chunk-local
                // count would misjudge the escape.
                if backtick_is_escaped(source, chunk.byte_offset.saturating_add(idx)) {
                    continue;
                }
                let message = "unclosed inline code span — pulldown-cmark could not pair \
                     this backtick with a closing fence"
                    .to_owned();
                if let Some(d) = Diagnostic::at(doc, chunk.byte_offset, idx..idx.saturating_add(1), message, None) {
                    out.push(d);
                }
            }
        }
    }
}

/// A byte is escaped iff preceded by an odd run of backslashes (`CommonMark`
/// §2.4). Mirrors `preceding_backslashes_even` in `mdwright-math`'s scanner;
/// the rule is a frozen part of the `CommonMark` spec, so the two copies cannot
/// drift. Extract to a shared crate only once a third caller needs it. `bytes`
/// must be the full source so a backslash run is never truncated at a boundary.
fn backtick_is_escaped(bytes: &[u8], i: usize) -> bool {
    let mut count = 0usize;
    let mut j = i;
    while j > 0 && bytes.get(j.saturating_sub(1)).copied() == Some(b'\\') {
        count = count.saturating_add(1);
        j = j.saturating_sub(1);
    }
    count % 2 == 1
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use mdwright_document::Document;

    use super::UnbalancedBacktick;
    use crate::rule_set::RuleSet;

    fn rules() -> Result<RuleSet> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(UnbalancedBacktick))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(rs)
    }

    #[test]
    fn escaped_backtick_is_not_flagged() -> Result<()> {
        // `\`` is a CommonMark literal backtick, not an unpaired span.
        let doc = Document::parse(r"Use \`ls\` to list files.")?;
        let diags = rules()?.check(&doc);
        assert!(diags.is_empty(), "escaped backticks should not flag: {diags:?}");
        Ok(())
    }

    #[test]
    fn genuine_unpaired_backtick_is_flagged() -> Result<()> {
        // A real stray backtick in prose must still be reported.
        let doc = Document::parse("a stray ` backtick here\n")?;
        let diags = rules()?.check(&doc);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        Ok(())
    }

    #[test]
    fn even_backslashes_leave_backtick_unescaped() -> Result<()> {
        // `\\` is an escaped backslash (literal `\`); the following `` ` ``
        // is a real, unpaired delimiter and must still be flagged.
        let doc = Document::parse(r"path C:\\` trailing")?;
        let diags = rules()?.check(&doc);
        assert_eq!(diags.len(), 1, "expected one diagnostic: {diags:?}");
        Ok(())
    }
}
