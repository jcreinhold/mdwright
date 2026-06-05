//! Table cell separators that abut running text with no space before
//! them, e.g. `| value| next |`.
//!
//! When a `|` cell separator directly follows plain text, pulldown (and
//! GitHub) drop the whole row's column alignment: a `--:`/`:--` column
//! stops emitting its `text-align` for that row, so the table renders
//! ragged. The same source shape blocks `mdwright fmt`'s `align` style,
//! because padding the cell would insert the missing space and so
//! change the parsed output. The structural fix is a space before the
//! separator.
//!
//! A pipe that follows a *closed* inline construct — a code span,
//! emphasis run, or link — parses cleanly and is left alone. Plain text
//! and a link both end in `)`, so the byte before the pipe cannot tell
//! the two apart; the rule instead asks the parser directly whether
//! inserting the space changes how the table parses.

use crate::diagnostic::{Diagnostic, Fix};
use crate::rule::LintRule;
use mdwright_document::{Document, MarkdownSignature, ParseOptions, TableAlign, markdown_signature};

pub struct TablePipeSpacing;

impl LintRule for TablePipeSpacing {
    fn name(&self) -> &str {
        "table-pipe-spacing"
    }

    fn description(&self) -> &str {
        "Table cell separator with no space before it, dropping the row's column alignment."
    }

    fn explain(&self) -> &str {
        include_str!("explain/table_pipe_spacing.md")
    }

    fn produces_fix(&self) -> bool {
        true
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        let source = doc.source();
        let bytes = source.as_bytes();
        let parse_options = doc.parse_options();
        for table in doc.table_sites() {
            // Only tables that declare an explicit column alignment have
            // anything to lose: a colon-free delimiter row renders the
            // same compact or spaced, so flagging it would be noise.
            if !table.alignments().iter().any(|a| !matches!(a, TableAlign::None)) {
                continue;
            }
            let table_range = table.raw_range();
            let Some(table_slice) = source.get(table_range.clone()) else {
                continue;
            };
            let Ok(baseline) = markdown_signature(table_slice, parse_options) else {
                continue;
            };
            for (row_idx, row) in table.rows().iter().enumerate() {
                // The delimiter row carries no cell content and parses
                // correctly even fully compact, so it never triggers the
                // alignment drop.
                if row_idx == 1 {
                    continue;
                }
                // Every cell except the last is followed by a separator
                // pipe at the end of its source range; the trailing edge
                // pipe (after the last cell) is not a separator.
                let Some((_last, leading)) = row.cells().split_last() else {
                    continue;
                };
                for cell in leading {
                    let pipe = cell.raw_range().end;
                    let abuts_content = pipe
                        .checked_sub(1)
                        .and_then(|i| bytes.get(i).copied())
                        .is_some_and(|b| !b.is_ascii_whitespace());
                    if !abuts_content {
                        continue;
                    }
                    let Some(rel_pipe) = pipe.checked_sub(table_range.start) else {
                        continue;
                    };
                    // The abutting separator is only a defect if it is
                    // why the row loses its alignment: the space pulldown
                    // needs must change how the table parses. A closed
                    // inline span before the pipe parses cleanly, leaving
                    // the signature unchanged, and is not flagged.
                    if !separator_space_changes_parse(table_slice, rel_pipe, parse_options, &baseline) {
                        continue;
                    }
                    let message = "table cell separator is not preceded by a space — the renderer \
                         drops this row's column alignment; insert a space before the `|`"
                        .to_owned();
                    let fix = Some(Fix {
                        replacement: " ".to_owned(),
                        safe: true,
                    });
                    if let Some(d) = Diagnostic::at(doc, pipe, 0..0, message, fix) {
                        out.push(d);
                    }
                }
            }
        }
    }
}

/// Whether inserting a space before the separator at `rel_pipe` (a byte
/// offset into `table_slice`) changes the table's parsed signature.
fn separator_space_changes_parse(
    table_slice: &str,
    rel_pipe: usize,
    opts: ParseOptions,
    baseline: &MarkdownSignature,
) -> bool {
    let (Some(head), Some(tail)) = (table_slice.get(..rel_pipe), table_slice.get(rel_pipe..)) else {
        return false;
    };
    let mut spaced = String::with_capacity(table_slice.len().saturating_add(1));
    spaced.push_str(head);
    spaced.push(' ');
    spaced.push_str(tail);
    markdown_signature(&spaced, opts).is_ok_and(|sig| &sig != baseline)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use mdwright_document::Document;

    use super::TablePipeSpacing;
    use crate::apply_safe_fixes;
    use crate::rule_set::RuleSet;

    fn rules() -> Result<RuleSet> {
        let mut rs = RuleSet::new();
        rs.add(Box::new(TablePipeSpacing)).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(rs)
    }

    fn diagnostic_count(src: &str) -> Result<usize> {
        Ok(rules()?.check(&Document::parse(src)?).len())
    }

    #[test]
    fn flags_body_cell_separator_without_leading_space() -> Result<()> {
        let src = "| File | Words |\n| --- | ---: |\n| a.md| 1.7k |\n";
        assert_eq!(diagnostic_count(src)?, 1);
        Ok(())
    }

    #[test]
    fn flags_header_cell_separator_without_leading_space() -> Result<()> {
        let src = "| File| Words |\n| --- | ---: |\n| a.md | 1.7k |\n";
        assert_eq!(diagnostic_count(src)?, 1);
        Ok(())
    }

    #[test]
    fn ignores_well_spaced_aligned_table() -> Result<()> {
        let src = "| File | Words |\n| --- | ---: |\n| a.md | 1.7k |\n";
        assert_eq!(diagnostic_count(src)?, 0);
        Ok(())
    }

    #[test]
    fn ignores_compact_delimiter_row() -> Result<()> {
        // The delimiter row may be fully compact without dropping
        // alignment, so it must not be flagged.
        let src = "| File | Words |\n|---|---:|\n| a.md | 1.7k |\n";
        assert_eq!(diagnostic_count(src)?, 0);
        Ok(())
    }

    #[test]
    fn ignores_table_without_explicit_alignment() -> Result<()> {
        let src = "| File | Words |\n| --- | --- |\n| a.md| 1.7k |\n";
        assert_eq!(diagnostic_count(src)?, 0);
        Ok(())
    }

    #[test]
    fn ignores_escaped_pipe_inside_cell() -> Result<()> {
        let src = "| File | Words |\n| --- | ---: |\n| a\\|b | 1.7k |\n";
        assert_eq!(diagnostic_count(src)?, 0);
        Ok(())
    }

    #[test]
    fn ignores_code_span_before_separator() -> Result<()> {
        // A closed code span before the pipe parses cleanly and keeps
        // the row's alignment, so it is not a defect even without a
        // space.
        let src = "| File | Words |\n| --- | ---: |\n| `a.md`| 1.7k |\n";
        assert_eq!(diagnostic_count(src)?, 0);
        Ok(())
    }

    #[test]
    fn fix_inserts_space_before_separator() -> Result<()> {
        let src = "| File | Words |\n| --- | ---: |\n| a.md| 1.7k |\n";
        let doc = Document::parse(src)?;
        let diags = rules()?.check(&doc);
        let fix = diags
            .first()
            .and_then(|d| d.fix.as_ref())
            .ok_or_else(|| anyhow::anyhow!("fix"))?;
        assert!(fix.safe);
        assert_eq!(fix.replacement, " ");
        let (fixed, applied) = apply_safe_fixes(&doc, &diags);
        assert_eq!(applied, 1);
        assert_eq!(fixed, "| File | Words |\n| --- | ---: |\n| a.md | 1.7k |\n");
        let doc2 = Document::parse(&fixed)?;
        assert!(rules()?.check(&doc2).is_empty());
        Ok(())
    }
}
