use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mdwright_config::Config;
use mdwright_document::Document;
use mdwright_format::{FormatError, FormatReport, format_document, format_document_with_report, format_validated};
use mdwright_lint::RuleSet;

#[derive(Debug, Default)]
struct SoakStats {
    files_scanned: usize,
    parse_errors: usize,
    lint_diagnostics: usize,
    validation_errors: usize,
    idempotence_failures: usize,
    fmt_check_disagreements: usize,
    max_file_size: usize,
    rewrite_report: FormatReport,
    slowest: Vec<(Duration, PathBuf, usize)>,
}

impl SoakStats {
    fn record_timing(&mut self, elapsed: Duration, path: PathBuf, len: usize) {
        self.slowest.push((elapsed, path, len));
        self.slowest.sort_by_key(|entry| Reverse(entry.0));
        self.slowest.truncate(10);
    }

    fn has_failures(&self) -> bool {
        self.parse_errors > 0 || self.validation_errors > 0 || self.idempotence_failures > 0
    }
}

pub fn run(workspace: &Path, corpus_root: &Path) -> Result<bool> {
    let cfg = Config::discover(corpus_root).unwrap_or_else(|_| Config::defaults());
    let parse_options = cfg.parse_options();
    let fmt_options = cfg.fmt_options().clone();
    let rules = RuleSet::stdlib_defaults();
    let files = collect_inputs(workspace, corpus_root)?;
    let mut stats = SoakStats::default();

    for path in files {
        let start = Instant::now();
        let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let len = source.len();
        stats.files_scanned = stats.files_scanned.saturating_add(1);
        stats.max_file_size = stats.max_file_size.max(len);

        let doc = match Document::parse_with_options(&source, parse_options) {
            Ok(doc) => doc,
            Err(err) => {
                stats.parse_errors = stats.parse_errors.saturating_add(1);
                eprintln!("parse error: {}: {err}", path.display());
                stats.record_timing(start.elapsed(), path, len);
                continue;
            }
        };

        let diagnostics = rules.check(&doc);
        stats.lint_diagnostics = stats.lint_diagnostics.saturating_add(diagnostics.len());

        let (formatted, report) = format_document_with_report(&doc, &fmt_options);
        merge_report(&mut stats.rewrite_report, &report);
        if formatted != source {
            stats.fmt_check_disagreements = stats.fmt_check_disagreements.saturating_add(1);
        }

        match format_validated(&doc, &fmt_options) {
            Ok(_) => {}
            Err(FormatError::Parse(err)) => {
                stats.validation_errors = stats.validation_errors.saturating_add(1);
                eprintln!("format validation parse error: {}: {err}", path.display());
            }
            Err(FormatError::SemanticDivergence { diff_summary, .. }) => {
                stats.validation_errors = stats.validation_errors.saturating_add(1);
                eprintln!("format validation divergence: {}: {diff_summary}", path.display());
            }
        }

        match Document::parse_with_options(&formatted, parse_options) {
            Ok(formatted_doc) => {
                let twice = format_document(&formatted_doc, &fmt_options);
                if formatted != twice {
                    stats.idempotence_failures = stats.idempotence_failures.saturating_add(1);
                    eprintln!("format idempotence failure: {}", path.display());
                }
            }
            Err(err) => {
                stats.validation_errors = stats.validation_errors.saturating_add(1);
                eprintln!("formatted output parse error: {}: {err}", path.display());
            }
        }

        stats.record_timing(start.elapsed(), path, len);
    }

    print_report(&stats);
    Ok(!stats.has_failures())
}

fn collect_inputs(workspace: &Path, corpus_root: &Path) -> Result<Vec<PathBuf>> {
    let list_path = workspace.join("crates/mdwright/benches/corpus.list");
    let list = fs::read_to_string(&list_path).with_context(|| format!("read {}", list_path.display()))?;
    let mut files = Vec::new();
    for line in list.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        files.push(resolve_corpus_path(corpus_root, trimmed));
    }
    collect_markdown_files(&workspace.join("crates/mdwright/tests/external"), &mut files)?;
    files.sort();
    files.dedup();
    Ok(files)
}

fn resolve_corpus_path(corpus_root: &Path, rel: &str) -> PathBuf {
    let direct = corpus_root.join(rel);
    if direct.exists() {
        return direct;
    }
    let Some(stripped) = rel.strip_prefix("docs/books/") else {
        return direct;
    };
    let local_book = corpus_root.join(stripped);
    if local_book.exists() {
        return local_book;
    }
    if let Some(parent) = corpus_root.parent() {
        let sibling_book = parent.join(stripped);
        if sibling_book.exists() {
            return sibling_book;
        }
    }
    direct
}

fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry.with_context(|| format!("read entry under {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(())
}

fn merge_report(total: &mut FormatReport, report: &FormatReport) {
    total.rewrite_candidates = total.rewrite_candidates.saturating_add(report.rewrite_candidates);
    total.rewrite_committed = total.rewrite_committed.saturating_add(report.rewrite_committed);
    total.rewrite_rejected_overlap = total
        .rewrite_rejected_overlap
        .saturating_add(report.rewrite_rejected_overlap);
    total.rewrite_rejected_verification = total
        .rewrite_rejected_verification
        .saturating_add(report.rewrite_rejected_verification);
}

fn print_report(stats: &SoakStats) {
    println!("production soak summary");
    println!("  files scanned: {}", stats.files_scanned);
    println!("  parse errors: {}", stats.parse_errors);
    println!("  lint diagnostics: {}", stats.lint_diagnostics);
    println!("  format validation errors: {}", stats.validation_errors);
    println!("  format idempotence failures: {}", stats.idempotence_failures);
    println!("  fmt-check disagreements: {}", stats.fmt_check_disagreements);
    println!("  max file size: {} byte(s)", stats.max_file_size);
    println!(
        "  rewrite candidates attempted: {}",
        stats.rewrite_report.rewrite_candidates
    );
    println!(
        "  rewrite candidates committed: {}",
        stats.rewrite_report.rewrite_committed
    );
    println!(
        "  rewrite candidates rejected by overlap: {}",
        stats.rewrite_report.rewrite_rejected_overlap
    );
    println!(
        "  rewrite candidates rejected by verification: {}",
        stats.rewrite_report.rewrite_rejected_verification
    );
    println!("  slowest files:");
    for (elapsed, path, len) in &stats.slowest {
        println!("    {:>8.3?}  {:>8} byte(s)  {}", elapsed, len, path.display());
    }
}
