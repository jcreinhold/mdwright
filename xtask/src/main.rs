//! `cargo xtask doc-rules`         — regenerate or verify `docs/src/rules/*.md`.
//! `cargo xtask doc-cli`           — regenerate or verify `docs/src/reference/cli.md`.
//! `cargo xtask doc-config`        — regenerate or verify `docs/src/configuration.md`.
//! `cargo xtask bump-docs-version` — sync `vX.Y.Z` pins in integration docs to `Cargo.toml`.
//! `cargo xtask diagnose-fuzz`     — explain a libFuzzer crash artifact.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "mdwright maintenance tasks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Regenerate `docs/src/rules/<name>.md` and `docs/src/rules/index.md`.
    /// With `--check`, exit non-zero if any file would change.
    DocRules {
        /// Verify only; do not write.
        #[arg(long)]
        check: bool,
    },
    /// Regenerate `docs/src/reference/cli.md` from clap's `--help` output.
    /// With `--check`, exit non-zero if the file would change.
    DocCli {
        /// Verify only; do not write.
        #[arg(long)]
        check: bool,
    },
    /// Regenerate `docs/src/configuration.md` from the schema metadata
    /// in `xtask/src/config_docs.rs`. With `--check`, exit non-zero if
    /// the file would change.
    DocConfig {
        /// Verify only; do not write.
        #[arg(long)]
        check: bool,
    },
    /// Rewrite `rev: vX.Y.Z` and `@vX.Y.Z` pins in the integration
    /// docs to match `Cargo.toml`'s `version`. With `--check`, exit
    /// non-zero if any pin disagrees.
    BumpDocsVersion {
        /// Override the version to write. Defaults to `Cargo.toml`'s
        /// `[package].version`.
        #[arg(long)]
        version: Option<String>,
        /// Verify only; do not write. Ignores `--version` (always
        /// compares to `Cargo.toml`).
        #[arg(long)]
        check: bool,
    },
    /// Replay a libFuzzer crash artifact the way the fuzz target does,
    /// and print the option-byte decoding plus the
    /// `SemanticDivergence` summary (or whatever else the artifact
    /// surfaces). Read-only.
    DiagnoseFuzz {
        /// One or more paths to fuzz artifact files.
        #[arg(required = true)]
        artifacts: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("xtask: error: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let workspace = workspace_root()?;
    match cli.command {
        Command::DocRules { check } => {
            if check {
                let drift = xtask::check(&workspace)?;
                if drift.is_empty() {
                    Ok(ExitCode::SUCCESS)
                } else {
                    eprintln!("xtask: doc-rules drift detected ({} file(s)):", drift.len());
                    for d in &drift {
                        eprintln!("  {}", d.path.display());
                    }
                    eprintln!("  fix with: cargo xtask doc-rules");
                    Ok(ExitCode::from(1))
                }
            } else {
                xtask::regenerate(&workspace)?;
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::DocCli { check } => {
            if check {
                let drift = xtask::cli_docs::check(&workspace, None)?;
                if drift.is_empty() {
                    Ok(ExitCode::SUCCESS)
                } else {
                    eprintln!("xtask: doc-cli drift detected:");
                    for d in &drift {
                        eprintln!("  {}", d.path.display());
                    }
                    eprintln!("  fix with: cargo xtask doc-cli");
                    Ok(ExitCode::from(1))
                }
            } else {
                xtask::cli_docs::regenerate(&workspace, None)?;
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::DocConfig { check } => {
            if check {
                let drift = xtask::config_docs::check(&workspace)?;
                if drift.is_empty() {
                    Ok(ExitCode::SUCCESS)
                } else {
                    eprintln!("xtask: doc-config drift detected:");
                    for d in &drift {
                        eprintln!("  {}", d.path.display());
                    }
                    eprintln!("  fix with: cargo xtask doc-config");
                    Ok(ExitCode::from(1))
                }
            } else {
                xtask::config_docs::regenerate(&workspace)?;
                Ok(ExitCode::SUCCESS)
            }
        }
        Command::DiagnoseFuzz { artifacts } => {
            for (i, path) in artifacts.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                let diagnosis = xtask::diagnose_fuzz::diagnose(path)?;
                xtask::diagnose_fuzz::render(path, &diagnosis);
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::BumpDocsVersion { version, check } => {
            if check {
                let drift = xtask::version_refs::check(&workspace)?;
                if drift.is_empty() {
                    Ok(ExitCode::SUCCESS)
                } else {
                    eprintln!("xtask: bump-docs-version drift detected:");
                    for d in &drift {
                        eprintln!("  {}", d.path.display());
                    }
                    eprintln!("  fix with: cargo xtask bump-docs-version");
                    Ok(ExitCode::from(1))
                }
            } else {
                let v = match version {
                    Some(v) => v,
                    None => xtask::version_refs::current_version(&workspace)?,
                };
                xtask::version_refs::regenerate(&workspace, &v)?;
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

fn workspace_root() -> Result<PathBuf> {
    // `cargo xtask …` runs from the workspace root by default; in
    // case a future invocation runs from elsewhere, walk up looking
    // for the root `Cargo.toml` that declares `[workspace]`.
    let mut p = std::env::current_dir().context("read current directory")?;
    loop {
        if p.join("xtask").join("Cargo.toml").is_file() {
            return Ok(p);
        }
        if !p.pop() {
            anyhow::bail!("could not locate mdwright workspace root from $PWD");
        }
    }
}
