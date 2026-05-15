//! Non-hard-break trailing whitespace at the end of source lines.
//!
//! A line ending in `  \n` (two spaces) is a `CommonMark` hard break —
//! left alone. Any other run of trailing whitespace is a typo or a
//! leftover from a text editor: it's invisible, it churns diffs, and
//! `git apply` sometimes refuses it. Lines inside code blocks are
//! skipped — whitespace there can be meaningful.

use crate::diagnostic::{Diagnostic, Fix};
use crate::document::Document;
use crate::rule::LintRule;

pub struct TrailingWhitespace;

impl LintRule for TrailingWhitespace {
    fn name(&self) -> &str {
        "trailing-whitespace"
    }

    fn description(&self) -> &str {
        "Trailing whitespace at end of line."
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        let source = doc.source();
        let code_blocks = doc.code_blocks();
        let mut line_start: usize = 0;
        for line in source.split_inclusive('\n') {
            let line_end_incl = line_start.saturating_add(line.len());
            // Strip the trailing newline (if any) for whitespace
            // analysis; keep the original `\n` boundary for the
            // diagnostic span.
            let (content, newline_len) = line.strip_suffix('\n').map_or((line, 0), |c| (c, 1));
            let content_end = line_start.saturating_add(content.len());
            let trail = content
                .bytes()
                .rev()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            if trail == 0 {
                line_start = line_end_incl;
                continue;
            }
            // Hard-break: exactly two trailing spaces. Leave alone.
            if trail == 2 && content.bytes().rev().take(2).all(|b| b == b' ') && content.len() > 2 {
                line_start = line_end_incl;
                continue;
            }
            let span_start = content_end.saturating_sub(trail);
            let span_end = content_end;
            let inside_code = code_blocks
                .iter()
                .any(|c| c.raw_range.start <= span_start && span_start < c.raw_range.end);
            if inside_code {
                line_start = line_end_incl;
                continue;
            }
            let local = 0..(span_end.saturating_sub(span_start));
            if let Some(d) = Diagnostic::at(
                doc,
                span_start,
                local,
                "trailing whitespace".to_owned(),
                Some(Fix {
                    replacement: String::new(),
                    safe: true,
                }),
            ) {
                out.push(d);
            }
            // suppress unused-warning on newline_len without churn
            let _ = newline_len;
            line_start = line_end_incl;
        }
    }
}
