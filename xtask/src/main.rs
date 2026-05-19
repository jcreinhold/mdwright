//! `cargo xtask doc-rules`: regenerate or verify `docs/src/rules/*.md`.
//! `cargo xtask doc-cli`: regenerate or verify `docs/src/reference/cli.md`.
//! `cargo xtask doc-config`: regenerate or verify `docs/src/configuration.md`.
//! `cargo xtask bump-docs-version`: sync `vX.Y.Z` pins in integration docs to `Cargo.toml`.
//! `cargo xtask diagnose-fuzz`: explain a libFuzzer crash artifact.
//! `cargo xtask production-soak`: run release-oriented corpus checks.
//! `cargo xtask mdformat-parity`: compare mdwright and mdformat output over a corpus.
//! `cargo xtask parser-audit`: compare pulldown-cmark with cmark-gfm.
//! `cargo xtask release-evidence`: aggregate local release-candidate evidence.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

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
    /// Run parser/lint/format/idempotence checks over the release corpus.
    ProductionSoak {
        /// Corpus root containing paths from `crates/mdwright/benches/corpus.list`.
        #[arg(long)]
        corpus_root: PathBuf,
        /// Directory for JSON and Markdown reports.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Compare mdwright and mdformat output over a corpus.
    MdformatParity {
        /// Corpus directory or single Markdown file to copy into isolated formatter roots.
        #[arg(long)]
        corpus_root: PathBuf,
        /// Stable name used to match rows in `docs/architecture/mdformat-parity.md`.
        #[arg(long)]
        corpus_name: Option<String>,
        /// mdwright config to copy to the mdwright formatter root as `.mdwright.toml`.
        #[arg(long)]
        mdwright_config: PathBuf,
        /// mdformat config to copy to the mdformat formatter root as `.mdformat.toml`.
        #[arg(long)]
        mdformat_config: PathBuf,
        /// Directory for JSON and Markdown reports.
        #[arg(long, default_value = "target/mdwright/parity")]
        output: PathBuf,
        /// Keep the temporary formatter roots and print their path.
        #[arg(long)]
        keep_temp: bool,
        /// Append unclassified observed differences to the classification table as open bugs.
        #[arg(long)]
        bless_classification: bool,
        /// Require generated docs differences to be classified instead of ignored.
        #[arg(long)]
        include_generated: bool,
    },
    /// Compare mdwright's pulldown-cmark backend against cmark-gfm.
    ParserAudit {
        /// Cases to audit.
        #[arg(long, value_enum, default_value_t = ParserAuditCaseSet::All)]
        case_set: ParserAuditCaseSet,
        /// Directory for JSON and Markdown reports.
        #[arg(long, default_value = "target/mdwright/parser-audit")]
        output: PathBuf,
        /// Build a pinned cmark-gfm binary under `target/mdwright/tools`.
        #[arg(long)]
        ensure_tools: bool,
        /// Include comrak rendered-HTML and source-position diagnostics.
        #[arg(long)]
        include_comrak: bool,
        /// Use an explicit cmark-gfm binary instead of the pinned local tool.
        #[arg(long)]
        cmark_gfm_bin: Option<PathBuf>,
        /// Optional release corpus root to include with `--case-set corpus` or `all`.
        #[arg(long)]
        corpus_root: Option<PathBuf>,
    },
    /// Aggregate local release-candidate evidence into JSON and Markdown.
    ReleaseEvidence {
        /// Directory for JSON and Markdown reports.
        #[arg(long, default_value = "target/mdwright/release")]
        output: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ParserAuditCaseSet {
    GfmSpec,
    Corpus,
    All,
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
        Command::ProductionSoak { corpus_root, output } => {
            let output = output.as_deref().map(|path| {
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    workspace.join(path)
                }
            });
            if xtask::production_soak::run(&workspace, &corpus_root, output.as_deref())? {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        Command::MdformatParity {
            corpus_root,
            corpus_name,
            mdwright_config,
            mdformat_config,
            output,
            keep_temp,
            bless_classification,
            include_generated,
        } => {
            let opts = xtask::mdformat_parity::ParityOptions {
                corpus_root,
                corpus_name,
                mdwright_config,
                mdformat_config,
                output,
                keep_temp,
                bless_classification,
                include_generated,
            };
            if xtask::mdformat_parity::run(&workspace, opts)? {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        Command::ParserAudit {
            case_set,
            output,
            ensure_tools,
            include_comrak,
            cmark_gfm_bin,
            corpus_root,
        } => {
            let opts = xtask::parser_audit::ParserAuditOptions {
                case_set: match case_set {
                    ParserAuditCaseSet::GfmSpec => xtask::parser_audit::CaseSet::GfmSpec,
                    ParserAuditCaseSet::Corpus => xtask::parser_audit::CaseSet::Corpus,
                    ParserAuditCaseSet::All => xtask::parser_audit::CaseSet::All,
                },
                output,
                ensure_tools,
                include_comrak,
                cmark_gfm_bin,
                corpus_root,
            };
            if xtask::parser_audit::run(&workspace, opts)? {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
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
        Command::ReleaseEvidence { output } => {
            let output = if output.is_absolute() {
                output
            } else {
                workspace.join(output)
            };
            if xtask::release_evidence::run(&workspace, &output)? {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
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
