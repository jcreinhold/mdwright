//! Generator for `docs/src/rules/<name>.md` and `docs/src/rules/index.md`.
//!
//! The library has two entry points:
//!
//! - [`regenerate`] writes the doc tree on disk.
//! - [`check`] compares the on-disk tree against the expected output
//!   and returns a list of paths that drifted (empty = clean).
//!
//! The binary in `xtask/src/main.rs` is a clap wrapper around these
//! two functions; the integration test `tests/rule_docs_in_sync.rs`
//! calls [`check`] directly so the CI gate works without `cargo
//! xtask` on PATH.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mdwright::{LintRule, RuleSet};

pub mod cli_docs;
pub mod since;

/// Workspace-relative path to the per-rule docs directory.
pub const RULES_DIR: &str = "docs/src/rules";

/// One drifted file: its repo-relative path and the expected
/// contents. Returned by [`check`].
#[derive(Clone, Debug)]
pub struct Drift {
    pub path: PathBuf,
    pub expected: String,
}

/// Compose the expected contents of `docs/src/rules/<name>.md` for one
/// rule. The body is `frontmatter + "\n# <name>\n\n<description>\n\n
/// <explain>"`. If `explain()` is empty (third-party rules), the
/// body falls back to a stub directing the user at `list-rules`.
#[must_use]
pub fn page_for(rule: &dyn LintRule) -> String {
    let name = rule.name();
    let version = since::version_for(name).unwrap_or("unreleased");
    let frontmatter = format!(
        "---\n\
         name: {name}\n\
         default: {default}\n\
         advisory: {advisory}\n\
         fix: {fix}\n\
         since: {version}\n\
         ---\n",
        default = rule.is_default(),
        advisory = rule.is_advisory(),
        fix = rule.produces_fix(),
    );
    let body = rule.explain().trim();
    let body = if body.is_empty() {
        "_No long-form explanation available. Run `mdwright list-rules` for a one-line summary._".to_owned()
    } else {
        body.to_owned()
    };
    format!(
        "{frontmatter}\n\
         # {name}\n\n\
         {desc}\n\n\
         {body}\n",
        desc = rule.description()
    )
}

/// Compose the expected contents of `docs/src/rules/index.md` — one row
/// per stdlib rule with its description and a link to the page.
#[must_use]
pub fn index_page(rules: &RuleSet) -> String {
    let mut out = String::from("# Lint rules\n\n");
    out.push_str(
        "Every rule shipped by mdwright's standard library. Each link points to the rule's long-form\n\
         explanation; `mdwright explain <name>` prints the same text from the command line.\n\n",
    );
    out.push_str("| Rule | Default | Advisory | Fix | Description |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for rule in rules.iter() {
        out.push_str(&format!(
            "| [`{name}`]({path}) | {default} | {advisory} | {fix} | {desc} |\n",
            name = rule.name(),
            path = page_path_relative(rule.name()),
            default = yes_no(rule.is_default()),
            advisory = yes_no(rule.is_advisory()),
            fix = yes_no(rule.produces_fix()),
            desc = rule.description(),
        ));
    }
    out
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// Relative path of `<name>.md` from inside `docs/src/rules/`.
fn page_path_relative(name: &str) -> String {
    format!("{name}.md")
}

/// Absolute on-disk path for the rule page, anchored at `workspace`.
fn page_path(workspace: &Path, name: &str) -> PathBuf {
    let mut p = workspace.join(RULES_DIR);
    for component in name.split('/') {
        p.push(component);
    }
    p.set_extension("md");
    p
}

/// Rewrite every page on disk to match the current rule definitions.
///
/// # Errors
///
/// Surfaces I/O failures from creating subdirectories or writing files.
pub fn regenerate(workspace: &Path) -> Result<()> {
    let rules = RuleSet::stdlib_all();
    let rules_root = workspace.join(RULES_DIR);
    fs::create_dir_all(&rules_root).with_context(|| format!("create {}", rules_root.display()))?;

    for rule in rules.iter() {
        let path = page_path(workspace, rule.name());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let body = page_for(rule);
        fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    }

    let index_path = rules_root.join("index.md");
    fs::write(&index_path, index_page(&rules)).with_context(|| format!("write {}", index_path.display()))?;
    Ok(())
}

/// Compare every expected page (and the index) to its on-disk
/// counterpart. Returns a vector of [`Drift`] entries — empty means
/// no drift.
///
/// # Errors
///
/// Surfaces I/O failures other than `NotFound`; a missing file counts
/// as drift, not an error.
pub fn check(workspace: &Path) -> Result<Vec<Drift>> {
    let rules = RuleSet::stdlib_all();
    let mut drift = Vec::new();

    for rule in rules.iter() {
        let path = page_path(workspace, rule.name());
        let expected = page_for(rule);
        let actual = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
        };
        if actual != expected {
            drift.push(Drift { path, expected });
        }
    }

    let index_path = workspace.join(RULES_DIR).join("index.md");
    let expected_index = index_page(&rules);
    let actual_index = match fs::read_to_string(&index_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("read {}", index_path.display())),
    };
    if actual_index != expected_index {
        drift.push(Drift {
            path: index_path,
            expected: expected_index,
        });
    }

    Ok(drift)
}
