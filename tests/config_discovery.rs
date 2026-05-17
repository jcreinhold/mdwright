//! Discovery-cascade integration tests for [`Config::discover`].
//!
//! Each case lays out a directory tree with `tempfile::tempdir()`,
//! then calls `Config::discover(some_subdir)` and asserts the
//! resolved `rules_spec`. Discovery takes the start directory as a
//! parameter, so no test needs to chdir — they can all run in
//! parallel under nextest.

use std::fs;

use anyhow::{Result, anyhow};
use mdwright::Config;
use tempfile::tempdir;

fn write(path: &std::path::Path, contents: &str) -> Result<()> {
    fs::write(path, contents).map_err(|e| anyhow!("write {}: {e}", path.display()))
}

#[test]
fn discover_returns_defaults_when_no_file_anywhere() -> Result<()> {
    let dir = tempdir()?;
    // Plant a .git so the walk stops cleanly inside the tempdir
    // rather than escaping into the host's real config (if any).
    fs::create_dir(dir.path().join(".git"))?;
    let sub = dir.path().join("sub");
    fs::create_dir(&sub)?;
    let cfg = Config::discover(&sub).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(cfg.rules_spec(), "default");
    assert!(cfg.exclude_globs().is_empty());
    assert!(cfg.extra_info_strings().is_empty());
    Ok(())
}

#[test]
fn discover_finds_dot_mdwright_toml() -> Result<()> {
    let dir = tempdir()?;
    fs::create_dir(dir.path().join(".git"))?;
    let sub = dir.path().join("sub");
    fs::create_dir(&sub)?;
    write(
        &dir.path().join(".mdwright.toml"),
        "[lint]\nrules = \"unbalanced-backtick\"\n",
    )?;
    let cfg = Config::discover(&sub).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(cfg.rules_spec(), "unbalanced-backtick");
    Ok(())
}

#[test]
fn discover_finds_mdwright_toml_in_ancestor() -> Result<()> {
    let dir = tempdir()?;
    fs::create_dir(dir.path().join(".git"))?;
    let sub = dir.path().join("sub").join("nested");
    fs::create_dir_all(&sub)?;
    write(&dir.path().join("mdwright.toml"), "[lint]\nrules = \"bare-url\"\n")?;
    let cfg = Config::discover(&sub).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(cfg.rules_spec(), "bare-url");
    Ok(())
}

#[test]
fn discover_pyproject_with_tool_mdwright() -> Result<()> {
    let dir = tempdir()?;
    fs::create_dir(dir.path().join(".git"))?;
    let sub = dir.path().join("sub");
    fs::create_dir(&sub)?;
    write(
        &dir.path().join("pyproject.toml"),
        "[tool.mdwright.lint]\nrules = \"bare-url\"\n",
    )?;
    let cfg = Config::discover(&sub).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(cfg.rules_spec(), "bare-url");
    Ok(())
}

#[test]
fn discover_stops_at_git_boundary() -> Result<()> {
    // Layout:
    //   outer/mdwright.toml         (must NOT be picked up)
    //   outer/proj/.git/            (workspace boundary)
    //   outer/proj/sub/             (start dir)
    //
    // Discovery starting from `outer/proj/sub` walks upward, finds no
    // config in `sub` or `proj`, hits `.git/` inside `proj`, and stops
    // *before* visiting `outer/`.
    let outer = tempdir()?;
    let proj = outer.path().join("proj");
    let sub = proj.join("sub");
    fs::create_dir_all(&sub)?;
    fs::create_dir(proj.join(".git"))?;
    write(&outer.path().join("mdwright.toml"), "[lint]\nrules = \"bare-url\"\n")?;
    let cfg = Config::discover(&sub).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(
        cfg.rules_spec(),
        "default",
        "outer config must not leak past the .git/ boundary"
    );
    Ok(())
}

#[test]
fn discover_prefers_dotfile_over_plain_in_same_dir() -> Result<()> {
    let dir = tempdir()?;
    fs::create_dir(dir.path().join(".git"))?;
    write(
        &dir.path().join(".mdwright.toml"),
        "[lint]\nrules = \"unbalanced-backtick\"\n",
    )?;
    write(&dir.path().join("mdwright.toml"), "[lint]\nrules = \"bare-url\"\n")?;
    let cfg = Config::discover(dir.path()).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(
        cfg.rules_spec(),
        "unbalanced-backtick",
        ".mdwright.toml must win over mdwright.toml"
    );
    Ok(())
}

#[test]
fn discover_prefers_local_config_over_pyproject() -> Result<()> {
    let dir = tempdir()?;
    fs::create_dir(dir.path().join(".git"))?;
    write(
        &dir.path().join(".mdwright.toml"),
        "[lint]\nrules = \"unbalanced-backtick\"\n",
    )?;
    write(
        &dir.path().join("pyproject.toml"),
        "[tool.mdwright.lint]\nrules = \"bare-url\"\n",
    )?;
    let cfg = Config::discover(dir.path()).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(
        cfg.rules_spec(),
        "unbalanced-backtick",
        ".mdwright.toml must win over pyproject.toml [tool.mdwright]"
    );
    Ok(())
}

#[test]
fn discover_skips_pyproject_without_tool_table() -> Result<()> {
    // Layout:
    //   root/.git/
    //   root/mdwright.toml          (rules = "bare-url")
    //   root/proj/pyproject.toml    (no [tool.mdwright])
    //   root/proj/sub/              (start dir)
    //
    // The pyproject in `proj/` lacks `[tool.mdwright]`, so it must not
    // stop the walk; discovery should continue and find the outer
    // `mdwright.toml`.
    let root = tempdir()?;
    fs::create_dir(root.path().join(".git"))?;
    let proj = root.path().join("proj");
    let sub = proj.join("sub");
    fs::create_dir_all(&sub)?;
    write(&root.path().join("mdwright.toml"), "[lint]\nrules = \"bare-url\"\n")?;
    write(&proj.join("pyproject.toml"), "[project]\nname = \"unrelated\"\n")?;
    let cfg = Config::discover(&sub).map_err(|e| anyhow!("discover: {e}"))?;
    assert_eq!(cfg.rules_spec(), "bare-url");
    Ok(())
}

#[test]
fn load_explicit_reads_the_given_path() -> Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("custom.toml");
    write(&path, "[lint]\nrules = \"bare-url\"\n")?;
    let cfg = Config::load_explicit(&path).map_err(|e| anyhow!("load_explicit: {e}"))?;
    assert_eq!(cfg.rules_spec(), "bare-url");
    Ok(())
}

#[test]
fn load_explicit_errors_on_missing_path() -> Result<()> {
    match Config::load_explicit(std::path::Path::new("/does/not/exist/mdwright.toml")) {
        Ok(_) => Err(anyhow!("missing file must error")),
        Err(err) => {
            let rendered = err.to_string();
            assert!(
                rendered.contains("/does/not/exist/mdwright.toml"),
                "error must mention the offending path: {rendered}"
            );
            Ok(())
        }
    }
}
