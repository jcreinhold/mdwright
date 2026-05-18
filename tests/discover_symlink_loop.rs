//! Pins the symlink-safety contract: `discover_markdown` must
//! terminate on a symlink loop and must not descend through symlinks
//! into out-of-tree files. The current policy is `follow_links(false)`
//! (see `src/discover.rs`), which makes loops unreachable; this test
//! traps any future flip of that flag without an accompanying
//! loop-detection mechanism.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::symlink;

use mdwright_cli::discover_markdown;
use tempfile::tempdir;

#[test]
fn symlink_cycle_does_not_hang() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();

    // Real Markdown file at the root — discover should find this and
    // nothing else, regardless of the symlink cycle below it.
    fs::write(root.join("real.md"), "real\n")?;

    // a/ and b/ each exist, with a/loop -> ../b and b/loop -> ../a.
    // Following these naively would recurse forever; the walker must
    // either refuse to follow or detect the loop.
    let a = root.join("a");
    let b = root.join("b");
    fs::create_dir(&a)?;
    fs::create_dir(&b)?;
    symlink("../b", a.join("loop"))?;
    symlink("../a", b.join("loop"))?;
    fs::write(a.join("inside_a.md"), "a\n")?;
    fs::write(b.join("inside_b.md"), "b\n")?;

    // With follow_links(false), every Markdown file under the real
    // directory structure is enumerated exactly once; symlinks are
    // ignored. This call must terminate quickly.
    let found = discover_markdown(root);
    let names: Vec<String> = found
        .iter()
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_owned))
        .collect();
    assert!(names.contains(&"real.md".to_owned()), "found = {names:?}");
    assert!(names.contains(&"inside_a.md".to_owned()), "found = {names:?}");
    assert!(names.contains(&"inside_b.md".to_owned()), "found = {names:?}");
    // No duplicates from cycle traversal.
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicates found: {names:?}");
    Ok(())
}

#[test]
fn self_symlink_terminates() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = dir.path();
    fs::write(root.join("real.md"), "real\n")?;
    symlink(".", root.join("self"))?;
    let found = discover_markdown(root);
    assert!(
        found
            .iter()
            .any(|p| p.file_name().and_then(|s| s.to_str()) == Some("real.md"))
    );
    Ok(())
}
