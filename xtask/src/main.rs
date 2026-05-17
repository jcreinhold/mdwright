//! `cargo xtask doc-rules` — regenerate or verify `docs/rules/*.md`.

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
    /// Regenerate `docs/rules/<name>.md` and `docs/rules/index.md`.
    /// With `--check`, exit non-zero if any file would change.
    DocRules {
        /// Verify only; do not write.
        #[arg(long)]
        check: bool,
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
