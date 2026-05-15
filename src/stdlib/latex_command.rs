//! `\foo` LaTeX control sequence in prose.
//!
//! Opinionated rule: this project's convention is Unicode
//! mathematics in prose; LaTeX commands like `\cdot` do not render.
//! For commands with a known Unicode equivalent the rule offers a
//! safe autofix; commands taking an argument (`\bar{x}`) are flagged
//! without a fix. Default-off because other projects render LaTeX.

use std::sync::OnceLock;

use regex::Regex;

use crate::diagnostic::{Diagnostic, Fix};
use crate::document::Document;
use crate::rule::LintRule;
use crate::stdlib::latex_unicode::latex_unicode;
use crate::util::regex::compile_static;

pub struct LatexCommand;

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| compile_static(r"\\\\?([A-Za-z]+)(?:\{[^}]*\})?"))
}

impl LintRule for LatexCommand {
    fn name(&self) -> &str {
        "latex-command"
    }

    fn description(&self) -> &str {
        "LaTeX control sequence in prose (opt-in for Unicode-math projects)."
    }

    fn is_default(&self) -> bool {
        false
    }

    fn check(&self, doc: &Document<'_>, out: &mut Vec<Diagnostic>) {
        let math = doc.math_regions();
        for chunk in doc.prose_chunks() {
            for cap in pattern().captures_iter(chunk.text) {
                let Some(m) = cap.get(0) else { continue };
                let Some(name_match) = cap.get(1) else {
                    continue;
                };
                let abs_start = chunk.byte_offset.saturating_add(m.start());
                let abs_end = chunk.byte_offset.saturating_add(m.end());
                // Skip LaTeX commands inside math regions: `\alpha`
                // inside `\[ … \]` is intentional, not a Unicode-math
                // convention violation. The rule applies only to
                // prose context.
                if math
                    .iter()
                    .any(|r| r.range.start <= abs_start && abs_end <= r.range.end)
                {
                    continue;
                }
                let name = name_match.as_str();
                let fix = latex_unicode(name).map(|u| Fix {
                    replacement: u.to_owned(),
                    safe: true,
                });
                let message = format!(
                    "LaTeX command `\\{name}` in prose — replace with Unicode math; \
                     this project does not render LaTeX"
                );
                if let Some(d) = Diagnostic::at(doc, chunk.byte_offset, m.range(), message, fix) {
                    out.push(d);
                }
            }
        }
    }
}
