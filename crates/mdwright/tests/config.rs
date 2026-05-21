//! End-to-end tests for the `mdwright.toml` configuration file.
//!
//! Invokes the compiled binary via `CARGO_BIN_EXE_mdwright` and asserts
//! that the discovery cascade, `[lint]` rule selection, and `[lint] exclude`
//! actually influence what `check` reports.

use std::fs;
use std::process::Command;

use anyhow::{Result, anyhow};
use mdwright_config::Config;
use tempfile::tempdir;

fn run_check(args: &[&str]) -> Result<(bool, String)> {
    let bin = env!("CARGO_BIN_EXE_mdwright");
    let output = Command::new(bin).args(args).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    Ok((output.status.success(), stdout))
}

fn run_mdwright(args: &[&str]) -> Result<(bool, String, String)> {
    let bin = env!("CARGO_BIN_EXE_mdwright");
    let output = Command::new(bin).args(args).output()?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    Ok((output.status.success(), stdout, stderr))
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
    fs::write(&toml, "[lint]\npreset = \"none\"\nselect = [\"unbalanced-backtick\"]\n")?;
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
    fs::write(&toml, "[lint]\npreset = \"none\"\nselect = [\"unbalanced-backtick\"]\n")?;
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

#[test]
fn config_init_writes_documented_default_config() -> Result<()> {
    let dir = tempdir()?;
    let target = dir.path().join(".mdwright.toml");
    let target_arg = target.to_str().ok_or_else(|| anyhow!("non-utf8 path"))?;

    let (ok, stdout, stderr) = run_mdwright(&["config", "init", "--path", target_arg])?;
    assert!(ok, "config init should succeed: stdout={stdout} stderr={stderr}");
    let body = fs::read_to_string(&target)?;
    assert!(body.contains("[lint]"), "template should include lint table: {body}");
    assert!(
        body.contains("[fmt.math]"),
        "template should include math table: {body}"
    );
    assert!(
        body.contains("[parse.extensions.gfm]"),
        "template should include GFM extension table: {body}"
    );
    Config::load_explicit(&target).map_err(|e| anyhow!("load generated config: {e}"))?;

    let before = body;
    let (ok, _stdout, stderr) = run_mdwright(&["config", "init", "--path", target_arg])?;
    assert!(!ok, "second init without --force should fail");
    assert!(
        stderr.contains("--force"),
        "error should mention force override: {stderr}"
    );
    assert_eq!(fs::read_to_string(&target)?, before, "failed init must not rewrite");

    fs::write(&target, "sentinel\n")?;
    let (ok, stdout, stderr) = run_mdwright(&["config", "init", "--path", target_arg, "--force"])?;
    assert!(
        ok,
        "config init --force should succeed: stdout={stdout} stderr={stderr}"
    );
    let forced = fs::read_to_string(&target)?;
    assert_ne!(forced, "sentinel\n");
    Config::load_explicit(&target).map_err(|e| anyhow!("load forced config: {e}"))?;
    Ok(())
}
