//! Generator for `docs/src/configuration.md`.
//!
//! The rendered page comes from `mdwright-config`, which owns the
//! schema and the prose metadata for each TOML key. Drift is gated in
//! CI by `tests/config_docs_in_sync.rs`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use mdwright_config::documentation;

use crate::Drift;

/// Workspace-relative path to the rendered configuration reference.
pub const CONFIG_DOC_PATH: &str = "docs/src/configuration.md";

/// Build the expected contents of [`CONFIG_DOC_PATH`].
#[must_use]
pub fn render() -> String {
    documentation::render_reference_markdown()
}

/// Write the rendered page to disk.
///
/// # Errors
///
/// Surfaces I/O failures from creating the parent directory or
/// writing the file.
pub fn regenerate(workspace: &Path) -> Result<()> {
    let body = render();
    let path: PathBuf = workspace.join(CONFIG_DOC_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Compare the rendered page to its on-disk counterpart. Returns a
/// vector of [`Drift`] entries; empty means no drift.
///
/// # Errors
///
/// Surfaces I/O failures other than `NotFound`; a missing file counts
/// as drift, not an error.
pub fn check(workspace: &Path) -> Result<Vec<Drift>> {
    let expected = render();
    let path: PathBuf = workspace.join(CONFIG_DOC_PATH);
    let actual = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    if actual != expected {
        Ok(vec![Drift { path, expected }])
    } else {
        Ok(Vec::new())
    }
}
