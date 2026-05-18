//! Fenced code block info string not in a known-languages
//! allowlist.
//!
//! Default allowlist covers the languages this project uses;
//! advisory because every project has its own list, and the rule
//! exists to catch typos like ` ```sytax-error` rather than to
//! enforce a closed set.

use crate::diagnostic::Diagnostic;
use crate::rule::LintRule;
use mdwright_document::Document;

/// Languages we treat as definitely-fine info strings. The allowlist
/// is intentionally generous — false positives here are noise, not
/// errors.
const DEFAULT_ALLOWLIST: &[&str] = &[
    "",
    "text",
    "plain",
    "plaintext",
    "txt",
    "no-highlight",
    "nohighlight",
    "rust",
    "rs",
    "python",
    "py",
    "lean",
    "lean4",
    "agda",
    "haskell",
    "hs",
    "ocaml",
    "ml",
    "c",
    "cpp",
    "c++",
    "cxx",
    "objc",
    "objective-c",
    "js",
    "javascript",
    "ts",
    "typescript",
    "jsx",
    "tsx",
    "json",
    "json5",
    "toml",
    "yaml",
    "yml",
    "ini",
    "sh",
    "bash",
    "zsh",
    "fish",
    "console",
    "shell-session",
    "shellsession",
    "diff",
    "patch",
    "md",
    "markdown",
    "mdx",
    "html",
    "xml",
    "svg",
    "css",
    "scss",
    "sass",
    "less",
    "sql",
    "graphql",
    "make",
    "makefile",
    "cmake",
    "dockerfile",
    "tex",
    "latex",
    "bibtex",
    "go",
    "java",
    "kotlin",
    "swift",
    "scala",
    "ruby",
    "rb",
    "perl",
    "lua",
    "r",
    "julia",
    "jl",
    "matlab",
    "fortran",
    "elm",
    "erlang",
    "elixir",
    "ex",
    "nix",
    "zig",
    "rust-toml",
];

pub struct InfoStringTypo {
    extra: Vec<String>,
}

impl InfoStringTypo {
    /// Default instance — only the stdlib allowlist applies.
    #[must_use]
    pub fn new() -> Self {
        Self { extra: Vec::new() }
    }

    /// Extend the allowlist with project-specific language tags
    /// (`promql`, `kdb`, …). The stdlib defaults still apply; these
    /// are additions. The CLI wires this from `[lint.info-strings]
    /// extra` in `mdwright.toml`.
    #[must_use]
    pub fn with_extra(extra: Vec<String>) -> Self {
        Self { extra }
    }
}

impl Default for InfoStringTypo {
    fn default() -> Self {
        Self::new()
    }
}

impl LintRule for InfoStringTypo {
    fn name(&self) -> &str {
        "info-string-typo"
    }

    fn description(&self) -> &str {
        "Fenced code block info string not in the known-languages allowlist."
    }

    fn explain(&self) -> &str {
        include_str!("explain/info_string_typo.md")
    }

    fn is_advisory(&self) -> bool {
        true
    }

    fn check(&self, doc: &Document, out: &mut Vec<Diagnostic>) {
        for cb in doc.code_blocks() {
            if !cb.fenced {
                continue;
            }
            let info: &str = cb.info.as_str();
            // Some renderers allow attributes after the language tag
            // (`rust,no_run`). Strip everything after the first comma
            // or whitespace before allowlist checking.
            let language = info.split([',', ' ', '\t']).next().unwrap_or("");
            let language_lower = language.to_ascii_lowercase();
            if DEFAULT_ALLOWLIST.iter().any(|&a| a == language_lower)
                || self.extra.iter().any(|e| e.eq_ignore_ascii_case(&language_lower))
            {
                continue;
            }
            let message = format!(
                "unfamiliar code-fence info string `{language}` — typo, or extend the \
                 allowlist if this is intentional"
            );
            // Point at the fence line — the first line of the block.
            let line_end = doc
                .source()
                .get(cb.raw_range.start..cb.raw_range.end)
                .and_then(|s| s.find('\n'))
                .map_or(cb.raw_range.end, |n| cb.raw_range.start.saturating_add(n));
            let local = 0..(line_end.saturating_sub(cb.raw_range.start));
            if let Some(d) = Diagnostic::at(doc, cb.raw_range.start, local, message, None) {
                out.push(d);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::InfoStringTypo;
    use crate::rule_set::RuleSet;
    use mdwright_document::Document;

    #[test]
    fn extra_allowlist_silences_known_language() -> Result<()> {
        let src = "```promql\nrate(http_requests_total[5m])\n```\n";
        // Without extra: the rule should fire because `promql` isn't
        // in the stdlib allowlist.
        let mut rs = RuleSet::new();
        rs.add(Box::new(InfoStringTypo::new()))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let baseline = rs.check(&Document::parse(src));
        assert!(
            baseline.iter().any(|d| d.rule == "info-string-typo"),
            "baseline should report info-string-typo; got {baseline:?}"
        );

        // With extra: silenced.
        let mut rs = RuleSet::new();
        rs.add(Box::new(InfoStringTypo::with_extra(vec!["promql".to_owned()])))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let extended = rs.check(&Document::parse(src));
        assert!(
            !extended.iter().any(|d| d.rule == "info-string-typo"),
            "extra allowlist should silence info-string-typo; got {extended:?}"
        );
        Ok(())
    }
}
