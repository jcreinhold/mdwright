//! File-system walker that enumerates Markdown files under one or
//! more roots, honouring `.gitignore` and skipping common
//! build/cache directories.
//!
//! Built on the `ignore` crate (used by ripgrep, fd, dprint), which
//! handles the awkward parts: nested `.gitignore`, parent-directory
//! ignore lookups, symbolic-link safety.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

// Safety policy: we do **not** follow symlinks. mdwright is run over
// repositories that may include developer-created or attacker-supplied
// symlinks; following them risks descending into infinite loops or
// out-of-tree files. Users with intentional symlinked directory layouts
// can `find -L | xargs mdwright`. See `tests/discover_symlink_loop.rs`.

/// Collect every Markdown file (`*.md`, `*.markdown`) under `root`,
/// or `root` itself if it is already a Markdown file. The result is
/// sorted for stable output and deduplicated.
#[must_use]
pub fn discover_markdown(root: &Path) -> Vec<PathBuf> {
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false) // include dotfiles; user may pass an explicit path
        .git_ignore(true)
        .git_exclude(true)
        .git_global(false)
        .follow_links(false)
        .filter_entry(|entry| {
            // Skip a handful of directories that are never source.
            // `ignore` will already skip these if a `.gitignore`
            // mentions them, but we also catch fresh checkouts.
            entry
                .file_name()
                .to_str()
                .is_none_or(|name| !is_skip_dir(name))
        });

    let mut out = Vec::new();
    for entry in walker.build().flatten() {
        let path = entry.path();
        if path.is_file() && is_markdown(path) {
            out.push(path.to_path_buf());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn is_markdown(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|s| s.to_str()),
        Some("md" | "markdown")
    )
}

fn is_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "target" | "node_modules" | ".venv" | ".lake" | ".ruff_cache" | ".pytest_cache" | ".cargo"
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;
    use tempfile::tempdir;

    use super::discover_markdown;

    #[test]
    fn finds_md_files_under_root() -> Result<()> {
        let dir = tempdir()?;
        let root = dir.path();
        fs::write(root.join("a.md"), "x")?;
        fs::write(root.join("b.markdown"), "y")?;
        fs::write(root.join("c.txt"), "z")?;
        let found = discover_markdown(root);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|p| p.ends_with("a.md")));
        assert!(found.iter().any(|p| p.ends_with("b.markdown")));
        Ok(())
    }

    #[test]
    fn single_file_returns_itself() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("doc.md");
        fs::write(&path, "x")?;
        let found = discover_markdown(&path);
        assert_eq!(found, vec![path]);
        Ok(())
    }
}
