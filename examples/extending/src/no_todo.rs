//! `no-todo-in-prose` — flags literal `TODO` markers in paragraph
//! text. The match is case-sensitive and only fires inside prose;
//! `TODO` inside code spans, fenced code blocks, math regions, and
//! HTML blocks is left alone.

use mdwright::{Diagnostic, Document, LintRule};

pub struct NoTodoInProse;

impl LintRule for NoTodoInProse {
    fn name(&self) -> &str {
        "no-todo-in-prose"
    }

    fn description(&self) -> &str {
        "Literal TODO in paragraph text"
    }

    fn explain(&self) -> &str {
        "TODOs in user-facing documentation are usually accidents. Track \
         pending work in an issue tracker, or suppress this rule with \
         `<!-- mdwright: allow no-todo-in-prose -->` if the TODO is \
         intentional."
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for slice in doc.prose_chunks() {
            for (offset, _) in slice.text.match_indices("TODO") {
                if let Some(d) = Diagnostic::at(
                    doc,
                    slice.byte_offset,
                    offset..offset + "TODO".len(),
                    "literal `TODO` in prose".to_owned(),
                    None,
                ) {
                    out.push(d);
                }
            }
        }
    }
}
