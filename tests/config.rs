//! End-to-end tests for the `mdwright.toml` configuration file.
//!
//! Invokes the compiled binary via `CARGO_BIN_EXE_mdwright` and asserts
//! that the discovery cascade, `[lint] rules`, and `[lint] exclude`
//! actually influence what `check` reports.

use std::fs;
use std::process::Command;

use anyhow::{Result, anyhow};
use tempfile::tempdir;

fn run_check(args: &[&str]) -> Result<(bool, String)> {
    let bin = env!("CARGO_BIN_EXE_mdwright");
    let output = Command::new(bin).args(args).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    Ok((output.status.success(), stdout))
}

#[test]
fn cli_honours_config_rules() -> Result<()> {
    let dir = tempdir()?;
    let toml = dir.path().join("mdwright.toml");
    let sample = dir.path().join("sample.md");
    // The sample triggers two diagnostics under stdlib defaults:
    //   * `unbalanced-backtick` on the dangling opener;
    //   * `heading-punctuation` on the trailing period.
    // The config narrows the active set to just `unbalanced-backtick`.
    fs::write(&toml, "[lint]\nrules = \"unbalanced-backtick\"\n")?;
    fs::write(&sample, "# Title.\n\nProse with a `dangling backtick.\n")?;

    let (_ok, out) = run_check(&[
        "check",
        sample.to_str().ok_or_else(|| anyhow!("non-utf8 path"))?,
        "--config",
        toml.to_str().ok_or_else(|| anyhow!("non-utf8 path"))?,
        "--format",
        "compact",
    ])?;
    assert!(
        out.contains("unbalanced-backtick"),
        "config-selected rule should still fire: {out}"
    );
    assert!(
        !out.contains("heading-punctuation"),
        "rules not in the config-selected set must be silent: {out}"
    );
    Ok(())
}

#[test]
fn cli_rules_flag_overrides_config() -> Result<()> {
    // Same fixture as above, but pass `--rules default` to override the
    // config's narrower selection. Both diagnostics should appear.
    let dir = tempdir()?;
    let toml = dir.path().join("mdwright.toml");
    let sample = dir.path().join("sample.md");
    fs::write(&toml, "[lint]\nrules = \"unbalanced-backtick\"\n")?;
    fs::write(&sample, "# Title.\n\nProse with a `dangling backtick.\n")?;

    let (_ok, out) = run_check(&[
        "check",
        sample.to_str().ok_or_else(|| anyhow!("non-utf8 path"))?,
        "--config",
        toml.to_str().ok_or_else(|| anyhow!("non-utf8 path"))?,
        "--rules",
        "default",
        "--format",
        "compact",
    ])?;
    assert!(
        out.contains("unbalanced-backtick"),
        "default set includes unbalanced-backtick: {out}"
    );
    assert!(
        out.contains("heading-punctuation"),
        "CLI --rules default must restore the full default set: {out}"
    );
    Ok(())
}

#[test]
fn cli_honours_config_exclude_globs() -> Result<()> {
    let dir = tempdir()?;
    let toml = dir.path().join("mdwright.toml");
    let included = dir.path().join("keep.md");
    let excluded_dir = dir.path().join("vendored");
    fs::create_dir(&excluded_dir)?;
    let excluded = excluded_dir.join("drop.md");

    // Each file has a heading with trailing punctuation — guaranteed
    // diagnostic under defaults. The exclude pattern should remove the
    // vendored one entirely.
    fs::write(&included, "# Keep me.\n")?;
    fs::write(&excluded, "# Drop me.\n")?;
    fs::write(&toml, "[lint]\nexclude = [\"vendored/**\"]\n")?;

    let (_ok, out) = run_check(&[
        "check",
        dir.path().to_str().ok_or_else(|| anyhow!("non-utf8 path"))?,
        "--config",
        toml.to_str().ok_or_else(|| anyhow!("non-utf8 path"))?,
        "--format",
        "compact",
    ])?;
    assert!(out.contains("keep.md"), "non-excluded file should appear: {out}");
    assert!(!out.contains("drop.md"), "excluded file must not appear: {out}");
    Ok(())
}
