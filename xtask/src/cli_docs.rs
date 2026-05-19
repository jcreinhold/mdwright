//! Generator for `docs/src/reference/cli.md`.
//!
//! The page is the concatenation of `mdwright --help` and one
//! `mdwright <subcommand> --help` block per subcommand. We shell out
//! to the locally-built binary so clap's renderer remains the single
//! source of truth.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::Drift;

/// Workspace-relative path to the rendered CLI reference.
pub const CLI_DOC_PATH: &str = "docs/src/reference/cli.md";

/// Subcommands rendered by the generator. The empty string represents
/// the top-level invocation (`mdwright --help`). Order matches the
/// order users see them in the top-level help.
const SUBCOMMANDS: &[&str] = &[
    "",
    "check",
    "fix",
    "fmt",
    "fmt-check",
    "render",
    "list-rules",
    "explain",
    "lsp",
];

/// Build the expected contents of [`CLI_DOC_PATH`] by invoking each
/// subcommand's `--help`. Pass `Some(path)` to use an already-built
/// binary (e.g. from `env!("CARGO_BIN_EXE_mdwright")` in tests);
/// pass `None` to build via `cargo build -p mdwright --bin mdwright`.
///
/// # Errors
///
/// Returns an error if the mdwright binary cannot be built or any
/// `--help` invocation fails.
pub fn render(workspace: &Path, binary_override: Option<&Path>) -> Result<String> {
    let bin: PathBuf = match binary_override {
        Some(p) => p.to_path_buf(),
        None => ensure_binary(workspace)?,
    };

    let mut out = String::from("# CLI reference\n\n");
    out.push_str(
        "Auto-generated from clap's `--help` output by `cargo xtask doc-cli`. Edit the CLI definition in\n\
         `crates/mdwright/src/cli.rs` (or the rule registry for `list-rules`); never edit this file by hand.\n",
    );

    for subcmd in SUBCOMMANDS {
        out.push('\n');
        let heading = if subcmd.is_empty() {
            "mdwright".to_owned()
        } else {
            format!("mdwright {subcmd}")
        };
        out.push_str(&format!("## `{heading}`\n\n"));

        let mut cmd = Command::new(&bin);
        if !subcmd.is_empty() {
            cmd.arg(subcmd);
        }
        cmd.arg("--help");
        cmd.env("NO_COLOR", "1");
        cmd.env("CLICOLOR", "0");
        cmd.env_remove("CLICOLOR_FORCE");

        let output = cmd
            .output()
            .with_context(|| format!("invoke `{} {subcmd} --help`", bin.display()))?;
        if !output.status.success() {
            bail!(
                "`{} {subcmd} --help` failed (status {}):\n{}",
                bin.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr),
            );
        }
        let help = String::from_utf8(output.stdout).with_context(|| format!("non-UTF8 help for `{subcmd}`"))?;
        out.push_str("```text\n");
        out.push_str(help.trim_end_matches('\n'));
        out.push_str("\n```\n");
    }

    Ok(out)
}

/// Write the rendered page to disk.
///
/// # Errors
///
/// Surfaces I/O failures from creating the parent directory or
/// writing the file.
pub fn regenerate(workspace: &Path, binary_override: Option<&Path>) -> Result<()> {
    let body = render(workspace, binary_override)?;
    let path = workspace.join(CLI_DOC_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Compare the rendered page to its on-disk counterpart. Returns a
/// vector of [`Drift`] entries — empty means no drift.
///
/// # Errors
///
/// Surfaces I/O failures other than `NotFound`; a missing file counts
/// as drift, not an error.
pub fn check(workspace: &Path, binary_override: Option<&Path>) -> Result<Vec<Drift>> {
    let expected = render(workspace, binary_override)?;
    let path = workspace.join(CLI_DOC_PATH);
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

/// Build the mdwright binary in the given workspace and return its
/// absolute path. Used when no override is provided.
fn ensure_binary(workspace: &Path) -> Result<PathBuf> {
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "mdwright", "--bin", "mdwright"])
        .current_dir(workspace)
        .status()
        .context("invoke `cargo build -p mdwright --bin mdwright`")?;
    if !status.success() {
        bail!("`cargo build -p mdwright --bin mdwright` exited with {status}");
    }

    let bin = workspace
        .join("target")
        .join("debug")
        .join(if cfg!(windows) { "mdwright.exe" } else { "mdwright" });
    if !bin.is_file() {
        bail!("mdwright binary not found at {}", bin.display());
    }
    Ok(bin)
}
