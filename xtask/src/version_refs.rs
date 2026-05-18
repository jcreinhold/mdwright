//! Keep version pins in docs synchronised with `Cargo.toml`.
//!
//! Several places in the integration documentation and example
//! configs reference a concrete mdwright release tag — `rev: v0.4.0`
//! in pre-commit configs, `uses: jcreinhold/mdwright@v0.4.0` in
//! GitHub Actions snippets, etc. When the crate's `version` bumps,
//! these references must bump in lockstep or new releases ship with
//! docs pointing at the previous tag.
//!
//! This module recognises two patterns:
//!
//! - `rev: v<MAJOR.MINOR.PATCH>` (pre-commit `repos:` entries)
//! - `@v<MAJOR.MINOR.PATCH>`     (GitHub Actions `uses:` references)
//!
//! [`regenerate`] rewrites every match in [`VERSIONED_DOC_PATHS`] to
//! a target version. [`check`] returns a [`Drift`] for each file that
//! disagrees with `Cargo.toml`'s declared `version`. The drift gate
//! `tests/integration_versions_in_sync.rs` calls [`check`] directly.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow};
use regex::Regex;

use crate::Drift;

/// Workspace-relative paths whose version pins must stay in sync.
/// New files that pin a version should be added here.
pub const VERSIONED_DOC_PATHS: &[&str] = &[
    "README.md",
    "docs/src/integration/pre-commit.md",
    "docs/src/integration/github-actions.md",
    "examples/downstream/.pre-commit-config.yaml",
];

/// Read the workspace `version = "..."` value from the root `Cargo.toml`.
///
/// # Errors
///
/// Returns an error if the manifest cannot be read or does not
/// declare `[workspace.package].version`.
pub fn current_version(workspace: &Path) -> Result<String> {
    let manifest_path = workspace.join("Cargo.toml");
    let manifest =
        std::fs::read_to_string(&manifest_path).with_context(|| format!("read {}", manifest_path.display()))?;
    let parsed: toml::Value =
        toml::from_str(&manifest).with_context(|| format!("parse {} as TOML", manifest_path.display()))?;
    parsed
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Cargo.toml has no [workspace.package].version"))
}

/// Rewrite every `rev: vX.Y.Z` and `@vX.Y.Z` pattern in `content`
/// to point at `version` (without the leading `v`). Other text is
/// returned unchanged.
#[must_use]
pub fn rewrite(content: &str, version: &str) -> String {
    let re = version_pattern();
    re.replace_all(content, |caps: &regex::Captures<'_>| {
        format!("{}v{version}", &caps["prefix"])
    })
    .into_owned()
}

/// Match either `rev: v` or `@v` followed by a `MAJOR.MINOR.PATCH`
/// version (with optional pre-release suffix). The leading prefix
/// is captured so the replacement can preserve it.
fn version_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?P<prefix>rev:\s+|@)v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?")
            .expect("hard-coded version pattern compiles")
    })
}

/// Rewrite every file in [`VERSIONED_DOC_PATHS`] to pin `version`.
///
/// # Errors
///
/// Surfaces I/O failures from reading or writing any target file.
pub fn regenerate(workspace: &Path, version: &str) -> Result<()> {
    for rel in VERSIONED_DOC_PATHS {
        let path = workspace.join(rel);
        let current = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let rewritten = rewrite(&current, version);
        if rewritten != current {
            std::fs::write(&path, rewritten).with_context(|| format!("write {}", path.display()))?;
        }
    }
    Ok(())
}

/// Compare every file in [`VERSIONED_DOC_PATHS`] against the version
/// declared in `Cargo.toml`. Returns one [`Drift`] per file whose
/// rewritten form differs from its on-disk contents.
///
/// # Errors
///
/// Surfaces I/O failures from reading the manifest or any target
/// file.
pub fn check(workspace: &Path) -> Result<Vec<Drift>> {
    let version = current_version(workspace)?;
    let mut drift = Vec::new();
    for rel in VERSIONED_DOC_PATHS {
        let path: PathBuf = workspace.join(rel);
        let current = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let expected = rewrite(&current, &version);
        if expected != current {
            drift.push(Drift { path, expected });
        }
    }
    Ok(drift)
}
