use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use mdwright_document::render_html;
use mdwright_format::semantically_equivalent;
use serde::Serialize;
use tempfile::TempDir;

const CLASSIFICATION_PATH: &str = "docs/architecture/mdformat-parity.md";
const REPORT_JSON: &str = "mdformat-parity.json";
const REPORT_MD: &str = "mdformat-parity.md";

const GENERATED_DOC_PATTERNS: &[&str] = &[
    "src/configuration.md",
    "src/reference/cli.md",
    "src/reference/diagnostic-schema.md",
    "src/rules/**",
];

#[derive(Clone, Debug)]
pub struct ParityOptions {
    pub corpus_root: PathBuf,
    pub corpus_name: Option<String>,
    pub mdwright_config: PathBuf,
    pub mdformat_config: PathBuf,
    pub output: PathBuf,
    pub keep_temp: bool,
    pub bless_classification: bool,
    pub include_generated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ParityReport {
    corpus: String,
    corpus_root: String,
    mdwright_config: String,
    mdformat_config: String,
    temp_root: String,
    stats: ParityStats,
    original_vs_mdwright: TreeDiffSummary,
    original_vs_mdformat: TreeDiffSummary,
    mdwright_vs_mdformat: TreeDiffSummary,
    differences: Vec<DifferenceReport>,
    ignored_generated_differences: Vec<String>,
    failures: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct ParityStats {
    markdown_files: usize,
    mdwright_changed_files: usize,
    mdformat_changed_files: usize,
    output_different_files: usize,
    classified_differences: usize,
    unclassified_differences: usize,
    ignored_generated_differences: usize,
    fixed_rows_still_observed: usize,
    open_bug_rows_observed: usize,
    mdwright_semantic_drift_failures: usize,
    mdwright_semantic_parse_errors: usize,
    mdformat_semantic_drift: usize,
    mdformat_semantic_parse_errors: usize,
    mdwright_html_drift_failures: usize,
    mdwright_html_parse_errors: usize,
    mdformat_html_drift: usize,
    mdformat_html_parse_errors: usize,
    mdwright_idempotence_failures: usize,
    mdformat_idempotence_failures: usize,
    mdwright_fmt_check_failed: bool,
    mdwright_format_failed: bool,
    mdformat_format_failed: bool,
    mdwright_mdbook_failed: bool,
    mdformat_mdbook_failed: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
struct TreeDiffSummary {
    changed_files: usize,
    inserted_lines: usize,
    deleted_lines: usize,
}

#[derive(Clone, Debug, Serialize)]
struct DifferenceReport {
    path: String,
    mdwright_changed: bool,
    mdformat_changed: bool,
    semantic: SemanticStatus,
    rendered_html: HtmlStatus,
    classification: Option<ClassificationReport>,
}

#[derive(Clone, Debug, Serialize)]
struct SemanticStatus {
    mdwright: SemanticOutcome,
    mdformat: SemanticOutcome,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "detail")]
enum SemanticOutcome {
    Equivalent,
    Drift,
    ParseError(String),
    NotChecked,
}

#[derive(Clone, Debug, Serialize)]
struct HtmlStatus {
    mdwright: HtmlOutcome,
    mdformat: HtmlOutcome,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", content = "detail")]
enum HtmlOutcome {
    Equivalent,
    Drift,
    ParseError(String),
    NotChecked,
}

#[derive(Clone, Debug, Serialize)]
struct ClassificationReport {
    construct: String,
    class: String,
    status: String,
    owner: String,
    resolution: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassificationRow {
    corpus: String,
    path_pattern: String,
    construct: String,
    class: String,
    status: String,
    owner: String,
    resolution: String,
}

#[derive(Debug)]
struct MountedCorpus {
    temp: TempDir,
    mount_name: String,
    mdwright_root: PathBuf,
    mdformat_root: PathBuf,
    original_corpus: PathBuf,
    mdwright_corpus: PathBuf,
    mdformat_corpus: PathBuf,
}

pub fn run(workspace: &Path, opts: ParityOptions) -> Result<bool> {
    let corpus_root = opts
        .corpus_root
        .canonicalize()
        .with_context(|| format!("canonicalize corpus root {}", opts.corpus_root.display()))?;
    let mdwright_config = opts
        .mdwright_config
        .canonicalize()
        .with_context(|| format!("canonicalize mdwright config {}", opts.mdwright_config.display()))?;
    let mdformat_config = opts
        .mdformat_config
        .canonicalize()
        .with_context(|| format!("canonicalize mdformat config {}", opts.mdformat_config.display()))?;
    let output = if opts.output.is_absolute() {
        opts.output.clone()
    } else {
        workspace.join(&opts.output)
    };

    let corpus = opts.corpus_name.clone().unwrap_or_else(|| corpus_label(&corpus_root));
    let mounted = mount_corpus(&corpus_root, &mdwright_config, &mdformat_config)?;
    let classifications = load_classifications(&workspace.join(CLASSIFICATION_PATH))?;

    let mut failures = Vec::new();
    if let Err(err) = run_mdwright(workspace, &mounted.mdwright_root, &mounted.mount_name) {
        failures.push(format!("mdwright format failed: {err:#}"));
    }
    let mdwright_after_first = read_markdown_map(&mounted.mdwright_corpus)?;
    if let Err(err) = run_mdwright(workspace, &mounted.mdwright_root, &mounted.mount_name) {
        failures.push(format!("mdwright second format failed: {err:#}"));
    }
    let mdwright_after_second = read_markdown_map(&mounted.mdwright_corpus)?;
    let mdwright_fmt_check = run_mdwright_fmt_check(workspace, &mounted.mdwright_root, &mounted.mount_name);

    if let Err(err) = run_mdformat(&mounted.mdformat_root, &mounted.mount_name) {
        failures.push(format!("mdformat failed: {err:#}"));
    }
    let mdformat_after_first = read_markdown_map(&mounted.mdformat_corpus)?;
    if let Err(err) = run_mdformat(&mounted.mdformat_root, &mounted.mount_name) {
        failures.push(format!("mdformat second format failed: {err:#}"));
    }
    let mdformat_after_second = read_markdown_map(&mounted.mdformat_corpus)?;

    let mdwright_book = build_mdbook_if_present(&mounted.mdwright_corpus);
    let mdformat_book = build_mdbook_if_present(&mounted.mdformat_corpus);

    let original = read_markdown_map(&mounted.original_corpus)?;
    let mdwright = mdwright_after_second;
    let mdformat = mdformat_after_second;
    let original_vs_mdwright = summarize_diff(&original, &mdwright);
    let original_vs_mdformat = summarize_diff(&original, &mdformat);
    let mdwright_vs_mdformat = summarize_diff(&mdwright, &mdformat);

    let mdwright_idempotence = changed_paths_between(&mdwright_after_first, &mdwright);
    let mdformat_idempotence = changed_paths_between(&mdformat_after_first, &mdformat);

    let mut stats = ParityStats {
        markdown_files: original.len(),
        mdwright_changed_files: original_vs_mdwright.changed_files,
        mdformat_changed_files: original_vs_mdformat.changed_files,
        output_different_files: mdwright_vs_mdformat.changed_files,
        mdwright_idempotence_failures: mdwright_idempotence.len(),
        mdformat_idempotence_failures: mdformat_idempotence.len(),
        mdwright_fmt_check_failed: mdwright_fmt_check.is_err(),
        mdwright_format_failed: failures.iter().any(|f| f.starts_with("mdwright")),
        mdformat_format_failed: failures.iter().any(|f| f.starts_with("mdformat")),
        mdwright_mdbook_failed: mdwright_book.is_err(),
        mdformat_mdbook_failed: mdformat_book.is_err(),
        ..ParityStats::default()
    };

    if let Err(err) = mdwright_book {
        failures.push(format!("mdwright mdBook build failed: {err:#}"));
    }
    if let Err(err) = mdformat_book {
        failures.push(format!("mdformat mdBook build failed: {err:#}"));
    }
    for path in &mdwright_idempotence {
        failures.push(format!("mdwright idempotence failure: {path}"));
    }
    for path in &mdformat_idempotence {
        failures.push(format!("mdformat idempotence failure: {path}"));
    }
    if let Err(err) = mdwright_fmt_check {
        failures.push(format!("mdwright fmt-check disagreement after formatting: {err:#}"));
    }

    let mut differences = Vec::new();
    let mut ignored_generated = Vec::new();
    for path in changed_paths_between(&mdwright, &mdformat) {
        if !opts.include_generated
            && is_generated_doc(&path)
            && find_classification(&classifications, &corpus, &path).is_none()
        {
            ignored_generated.push(path);
            continue;
        }
        let source = original.get(&path);
        let mdwright_out = mdwright.get(&path);
        let mdformat_out = mdformat.get(&path);
        let semantic = semantic_status(source, mdwright_out, mdformat_out);
        let rendered_html = html_status(source, mdwright_out, mdformat_out);
        match &semantic.mdwright {
            SemanticOutcome::Equivalent | SemanticOutcome::NotChecked => {}
            SemanticOutcome::Drift => {
                stats.mdwright_semantic_drift_failures = stats.mdwright_semantic_drift_failures.saturating_add(1);
                failures.push(format!("mdwright semantic drift: {path}"));
            }
            SemanticOutcome::ParseError(_) => {
                stats.mdwright_semantic_parse_errors = stats.mdwright_semantic_parse_errors.saturating_add(1);
                failures.push(format!("mdwright semantic parse error: {path}"));
            }
        }
        match &semantic.mdformat {
            SemanticOutcome::Equivalent | SemanticOutcome::NotChecked => {}
            SemanticOutcome::Drift => {
                stats.mdformat_semantic_drift = stats.mdformat_semantic_drift.saturating_add(1);
            }
            SemanticOutcome::ParseError(_) => {
                stats.mdformat_semantic_parse_errors = stats.mdformat_semantic_parse_errors.saturating_add(1);
            }
        }
        match &rendered_html.mdwright {
            HtmlOutcome::Equivalent | HtmlOutcome::NotChecked => {}
            HtmlOutcome::Drift => {
                stats.mdwright_html_drift_failures = stats.mdwright_html_drift_failures.saturating_add(1);
                failures.push(format!("mdwright rendered HTML drift: {path}"));
            }
            HtmlOutcome::ParseError(_) => {
                stats.mdwright_html_parse_errors = stats.mdwright_html_parse_errors.saturating_add(1);
                failures.push(format!("mdwright rendered HTML parse error: {path}"));
            }
        }
        match &rendered_html.mdformat {
            HtmlOutcome::Equivalent | HtmlOutcome::NotChecked => {}
            HtmlOutcome::Drift => {
                stats.mdformat_html_drift = stats.mdformat_html_drift.saturating_add(1);
            }
            HtmlOutcome::ParseError(_) => {
                stats.mdformat_html_parse_errors = stats.mdformat_html_parse_errors.saturating_add(1);
            }
        }
        let row = find_classification(&classifications, &corpus, &path);
        let classification = row.map(classification_report);
        match classification.as_ref() {
            Some(report) => {
                stats.classified_differences = stats.classified_differences.saturating_add(1);
                if report.status == "fixed" {
                    stats.fixed_rows_still_observed = stats.fixed_rows_still_observed.saturating_add(1);
                    failures.push(format!("difference marked fixed still observed: {path}"));
                }
                if report.status == "open-bug" {
                    stats.open_bug_rows_observed = stats.open_bug_rows_observed.saturating_add(1);
                    failures.push(format!("open parity bug still observed: {path}"));
                }
            }
            None => {
                stats.unclassified_differences = stats.unclassified_differences.saturating_add(1);
                failures.push(format!("unclassified mdwright/mdformat difference: {path}"));
            }
        }
        differences.push(DifferenceReport {
            path: path.clone(),
            mdwright_changed: source != mdwright_out,
            mdformat_changed: source != mdformat_out,
            semantic,
            rendered_html,
            classification,
        });
    }
    stats.ignored_generated_differences = ignored_generated.len();

    if opts.bless_classification {
        append_unclassified_rows(workspace, &corpus, &differences)?;
    }

    let report = ParityReport {
        corpus,
        corpus_root: corpus_root.display().to_string(),
        mdwright_config: mdwright_config.display().to_string(),
        mdformat_config: mdformat_config.display().to_string(),
        temp_root: mounted.temp.path().display().to_string(),
        stats,
        original_vs_mdwright,
        original_vs_mdformat,
        mdwright_vs_mdformat,
        differences,
        ignored_generated_differences: ignored_generated,
        failures,
    };
    write_reports(&output, &report)?;
    print_summary(&output, &report);

    let success = report.failures.is_empty();
    if opts.keep_temp {
        let path = mounted.temp.keep();
        println!("kept temp root: {}", path.display());
    }
    Ok(success)
}

fn run_mdwright(workspace: &Path, root: &Path, mount_name: &str) -> Result<()> {
    run_mdwright_command(workspace, root, "fmt", mount_name)
}

fn run_mdwright_fmt_check(workspace: &Path, root: &Path, mount_name: &str) -> Result<()> {
    run_mdwright_command(workspace, root, "fmt-check", mount_name)
}

fn run_mdwright_command(workspace: &Path, root: &Path, subcommand: &str, mount_name: &str) -> Result<()> {
    let manifest = workspace.join("Cargo.toml");
    run_command(
        Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
            .current_dir(root)
            .args([
                "run",
                "-q",
                "--manifest-path",
                &manifest.display().to_string(),
                "-p",
                "mdwright",
                "--",
                subcommand,
                mount_name,
            ]),
    )
}

fn run_mdformat(root: &Path, mount_name: &str) -> Result<()> {
    run_command(Command::new("uvx").current_dir(root).args([
        "--with",
        "mdformat-gfm",
        "--with",
        "mdformat-frontmatter",
        "--with",
        "mdformat-footnote",
        "--with",
        "mdformat-mkdocs",
        "mdformat",
        "--no-validate",
        mount_name,
    ]))
}

fn run_command(command: &mut Command) -> Result<()> {
    let display = format!("{command:?}");
    let output = command.output().with_context(|| format!("run {display}"))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "{display} exited with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn build_mdbook_if_present(corpus: &Path) -> Result<()> {
    if !corpus.join("book.toml").is_file() {
        return Ok(());
    }
    run_command(Command::new("mdbook").arg("build").arg(corpus))
}

fn mount_corpus(corpus_root: &Path, mdwright_config: &Path, mdformat_config: &Path) -> Result<MountedCorpus> {
    let temp = tempfile::Builder::new()
        .prefix("mdwright-mdformat-parity.")
        .tempdir()
        .context("create parity tempdir")?;
    let mount_name = mount_name(corpus_root);
    let original_root = temp.path().join("original");
    let mdwright_root = temp.path().join("mdwright");
    let mdformat_root = temp.path().join("mdformat");
    let original_corpus = original_root.join(&mount_name);
    let mdwright_corpus = mdwright_root.join(&mount_name);
    let mdformat_corpus = mdformat_root.join(&mount_name);

    fs::create_dir_all(&original_root).with_context(|| format!("create {}", original_root.display()))?;
    fs::create_dir_all(&mdwright_root).with_context(|| format!("create {}", mdwright_root.display()))?;
    fs::create_dir_all(&mdformat_root).with_context(|| format!("create {}", mdformat_root.display()))?;
    copy_corpus(corpus_root, &original_corpus)?;
    copy_corpus(corpus_root, &mdwright_corpus)?;
    copy_corpus(corpus_root, &mdformat_corpus)?;
    fs::copy(mdwright_config, mdwright_root.join(".mdwright.toml"))
        .with_context(|| format!("copy {}", mdwright_config.display()))?;
    fs::copy(mdformat_config, mdformat_root.join(".mdformat.toml"))
        .with_context(|| format!("copy {}", mdformat_config.display()))?;

    Ok(MountedCorpus {
        temp,
        mount_name,
        mdwright_root,
        mdformat_root,
        original_corpus,
        mdwright_corpus,
        mdformat_corpus,
    })
}

fn copy_corpus(src: &Path, dst: &Path) -> Result<()> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::copy(src, dst).with_context(|| format!("copy {} to {}", src.display(), dst.display()))?;
        return Ok(());
    }
    fs::create_dir_all(dst).with_context(|| format!("create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry.with_context(|| format!("read entry under {}", src.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name == "book" {
            continue;
        }
        let target = dst.join(name);
        if path.is_dir() {
            copy_corpus(&path, &target)?;
        } else if path.is_file() {
            fs::copy(&path, &target).with_context(|| format!("copy {} to {}", path.display(), target.display()))?;
        }
    }
    Ok(())
}

fn mount_name(corpus_root: &Path) -> String {
    corpus_root
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("corpus")
        .to_owned()
}

fn corpus_label(corpus_root: &Path) -> String {
    mount_name(corpus_root)
}

fn read_markdown_map(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    collect_markdown(root, root, &mut out)?;
    Ok(out)
}

fn collect_markdown(root: &Path, current: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
    if current.is_file() {
        if current.extension().and_then(|ext| ext.to_str()) == Some("md") {
            let key = relative_path(root, current)?;
            out.insert(
                key,
                fs::read_to_string(current).with_context(|| format!("read {}", current.display()))?,
            );
        }
        return Ok(());
    }
    for entry in fs::read_dir(current).with_context(|| format!("read {}", current.display()))? {
        let entry = entry.with_context(|| format!("read entry under {}", current.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name == "book" {
            continue;
        }
        if path.is_dir() {
            collect_markdown(root, &path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            let key = relative_path(root, &path)?;
            out.insert(
                key,
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
            );
        }
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(root)
        .with_context(|| format!("strip {} from {}", root.display(), path.display()))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn summarize_diff(left: &BTreeMap<String, String>, right: &BTreeMap<String, String>) -> TreeDiffSummary {
    let mut summary = TreeDiffSummary::default();
    for path in changed_paths_between(left, right) {
        summary.changed_files = summary.changed_files.saturating_add(1);
        let old = left.get(&path).map_or("", String::as_str);
        let new = right.get(&path).map_or("", String::as_str);
        let (deleted, inserted) = line_diff_counts(old, new);
        summary.deleted_lines = summary.deleted_lines.saturating_add(deleted);
        summary.inserted_lines = summary.inserted_lines.saturating_add(inserted);
    }
    summary
}

fn changed_paths_between(left: &BTreeMap<String, String>, right: &BTreeMap<String, String>) -> Vec<String> {
    let paths: BTreeSet<_> = left.keys().chain(right.keys()).cloned().collect();
    paths
        .into_iter()
        .filter(|path| left.get(path) != right.get(path))
        .collect()
}

fn line_diff_counts(old: &str, new: &str) -> (usize, usize) {
    let old_lines: Vec<_> = old.lines().collect();
    let new_lines: Vec<_> = new.lines().collect();
    let common = lcs_len(&old_lines, &new_lines);
    (
        old_lines.len().saturating_sub(common),
        new_lines.len().saturating_sub(common),
    )
}

fn lcs_len(left: &[&str], right: &[&str]) -> usize {
    let mut prev = vec![0usize; right.len() + 1];
    let mut curr = vec![0usize; right.len() + 1];
    for l in left {
        for (j, r) in right.iter().enumerate() {
            curr[j + 1] = if l == r {
                prev[j].saturating_add(1)
            } else {
                curr[j].max(prev[j + 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }
    prev[right.len()]
}

fn semantic_status(original: Option<&String>, mdwright: Option<&String>, mdformat: Option<&String>) -> SemanticStatus {
    SemanticStatus {
        mdwright: semantic_outcome(original, mdwright),
        mdformat: semantic_outcome(original, mdformat),
    }
}

fn semantic_outcome(original: Option<&String>, formatted: Option<&String>) -> SemanticOutcome {
    let (Some(original), Some(formatted)) = (original, formatted) else {
        return SemanticOutcome::NotChecked;
    };
    match semantically_equivalent(original, formatted) {
        Ok(true) => SemanticOutcome::Equivalent,
        Ok(false) => SemanticOutcome::Drift,
        Err(err) => SemanticOutcome::ParseError(err.to_string()),
    }
}

fn html_status(original: Option<&String>, mdwright: Option<&String>, mdformat: Option<&String>) -> HtmlStatus {
    HtmlStatus {
        mdwright: html_outcome(original, mdwright),
        mdformat: html_outcome(original, mdformat),
    }
}

fn html_outcome(original: Option<&String>, formatted: Option<&String>) -> HtmlOutcome {
    let (Some(original), Some(formatted)) = (original, formatted) else {
        return HtmlOutcome::NotChecked;
    };
    let original_html = match render_html(original) {
        Ok(html) => html,
        Err(err) => return HtmlOutcome::ParseError(err.to_string()),
    };
    let formatted_html = match render_html(formatted) {
        Ok(html) => html,
        Err(err) => return HtmlOutcome::ParseError(err.to_string()),
    };
    if normalize_rendered_html(&original_html) == normalize_rendered_html(&formatted_html) {
        HtmlOutcome::Equivalent
    } else {
        HtmlOutcome::Drift
    }
}

fn normalize_rendered_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_whitespace = false;
    for ch in html.chars() {
        if ch.is_ascii_whitespace() {
            in_whitespace = true;
        } else {
            if in_whitespace && !out.is_empty() {
                out.push(' ');
            }
            out.push(ch);
            in_whitespace = false;
        }
    }
    out.trim().to_owned()
}

fn load_classifications(path: &Path) -> Result<Vec<ClassificationRow>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<_> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().trim_matches('`').to_owned())
            .collect();
        if cells.len() < 7 || cells[0] == "Corpus" || cells.iter().all(|cell| cell.chars().all(|c| c == '-')) {
            continue;
        }
        let row = ClassificationRow {
            corpus: cells[0].clone(),
            path_pattern: cells[1].clone(),
            construct: cells[2].clone(),
            class: cells[3].clone(),
            status: cells[4].clone(),
            owner: cells[5].clone(),
            resolution: cells[6].clone(),
        };
        validate_status(&row.status).with_context(|| format!("invalid status for {}", row.path_pattern))?;
        rows.push(row);
    }
    Ok(rows)
}

fn validate_status(status: &str) -> Result<()> {
    if matches!(
        status,
        "fixed" | "configured" | "intentional-divergence" | "upstream-parser-limitation" | "open-bug"
    ) {
        Ok(())
    } else {
        bail!("unknown parity status `{status}`")
    }
}

fn find_classification<'a>(rows: &'a [ClassificationRow], corpus: &str, path: &str) -> Option<&'a ClassificationRow> {
    rows.iter()
        .find(|row| corpus_matches(&row.corpus, corpus) && path_matches(&row.path_pattern, path))
}

fn corpus_matches(pattern: &str, corpus: &str) -> bool {
    pattern == "*" || pattern == corpus
}

fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    pattern == path
}

fn classification_report(row: &ClassificationRow) -> ClassificationReport {
    ClassificationReport {
        construct: row.construct.clone(),
        class: row.class.clone(),
        status: row.status.clone(),
        owner: row.owner.clone(),
        resolution: row.resolution.clone(),
    }
}

fn is_generated_doc(path: &str) -> bool {
    GENERATED_DOC_PATTERNS.iter().any(|pattern| path_matches(pattern, path))
}

fn append_unclassified_rows(workspace: &Path, corpus: &str, differences: &[DifferenceReport]) -> Result<()> {
    let mut additions = String::new();
    for difference in differences
        .iter()
        .filter(|difference| difference.classification.is_none())
    {
        additions.push_str(&format!(
            "| {corpus} | `{}` | untriaged | potential-bug | open-bug | formatter | Newly observed by `cargo xtask mdformat-parity`; classify before release. |\n",
            difference.path
        ));
    }
    if additions.is_empty() {
        return Ok(());
    }
    let path = workspace.join(CLASSIFICATION_PATH);
    let mut text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&additions);
    fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_reports(output: &Path, report: &ParityReport) -> Result<()> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    let json_path = output.join(REPORT_JSON);
    let md_path = output.join(REPORT_MD);
    fs::write(
        &json_path,
        serde_json::to_string_pretty(report).context("serialize parity report")?,
    )
    .with_context(|| format!("write {}", json_path.display()))?;
    fs::write(&md_path, markdown_report(report)).with_context(|| format!("write {}", md_path.display()))?;
    Ok(())
}

fn markdown_report(report: &ParityReport) -> String {
    let mut out = String::new();
    out.push_str("# mdformat parity report\n\n");
    out.push_str(&format!("- Corpus: `{}`\n", report.corpus));
    out.push_str(&format!("- Corpus root: `{}`\n", report.corpus_root));
    out.push_str(&format!("- mdwright config: `{}`\n", report.mdwright_config));
    out.push_str(&format!("- mdformat config: `{}`\n", report.mdformat_config));
    out.push_str(&format!("- Markdown files: `{}`\n", report.stats.markdown_files));
    out.push_str(&format!(
        "- mdwright changed files: `{}`\n",
        report.stats.mdwright_changed_files
    ));
    out.push_str(&format!(
        "- mdformat changed files: `{}`\n",
        report.stats.mdformat_changed_files
    ));
    out.push_str(&format!(
        "- mdwright/mdformat different files: `{}`\n",
        report.stats.output_different_files
    ));
    out.push_str(&format!(
        "- Unclassified differences: `{}`\n",
        report.stats.unclassified_differences
    ));
    out.push_str(&format!(
        "- Ignored generated differences: `{}`\n",
        report.stats.ignored_generated_differences
    ));
    out.push_str(&format!(
        "- mdwright semantic drift failures: `{}`\n",
        report.stats.mdwright_semantic_drift_failures
    ));
    out.push_str(&format!(
        "- mdwright semantic parse errors: `{}`\n",
        report.stats.mdwright_semantic_parse_errors
    ));
    out.push_str(&format!(
        "- mdformat semantic drift: `{}`\n",
        report.stats.mdformat_semantic_drift
    ));
    out.push_str(&format!(
        "- mdformat semantic parse errors: `{}`\n",
        report.stats.mdformat_semantic_parse_errors
    ));
    out.push_str(&format!(
        "- mdwright rendered HTML drift failures: `{}`\n",
        report.stats.mdwright_html_drift_failures
    ));
    out.push_str(&format!(
        "- mdwright rendered HTML parse errors: `{}`\n",
        report.stats.mdwright_html_parse_errors
    ));
    out.push_str(&format!(
        "- mdformat rendered HTML drift: `{}`\n",
        report.stats.mdformat_html_drift
    ));
    out.push_str(&format!(
        "- mdformat rendered HTML parse errors: `{}`\n",
        report.stats.mdformat_html_parse_errors
    ));
    out.push_str(&format!(
        "- mdwright fmt-check failed: `{}`\n",
        yes_no(report.stats.mdwright_fmt_check_failed)
    ));
    out.push_str("\n## Differences\n\n");
    out.push_str(
        "| Path | mdwright changed | mdformat changed | Semantic | Rendered HTML | Status | Class | Resolution |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for difference in &report.differences {
        let semantic = format!(
            "mdwright: {}; mdformat: {}",
            semantic_outcome_label(&difference.semantic.mdwright),
            semantic_outcome_label(&difference.semantic.mdformat)
        );
        let rendered_html = format!(
            "mdwright: {}; mdformat: {}",
            html_outcome_label(&difference.rendered_html.mdwright),
            html_outcome_label(&difference.rendered_html.mdformat)
        );
        let (status, class, resolution) = match &difference.classification {
            Some(c) => (c.status.as_str(), c.class.as_str(), c.resolution.as_str()),
            None => ("unclassified", "", ""),
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            difference.path,
            yes_no(difference.mdwright_changed),
            yes_no(difference.mdformat_changed),
            semantic,
            rendered_html,
            status,
            class,
            resolution
        ));
    }
    if !report.ignored_generated_differences.is_empty() {
        out.push_str("\n## Ignored generated differences\n\n");
        for path in &report.ignored_generated_differences {
            out.push_str(&format!("- `{path}`\n"));
        }
    }
    if !report.failures.is_empty() {
        out.push_str("\n## Failures\n\n");
        for failure in &report.failures {
            out.push_str(&format!("- {failure}\n"));
        }
    }
    out
}

fn print_summary(output: &Path, report: &ParityReport) {
    println!("mdformat parity summary");
    println!("  corpus: {}", report.corpus);
    println!("  markdown files: {}", report.stats.markdown_files);
    println!("  mdwright changed files: {}", report.stats.mdwright_changed_files);
    println!("  mdformat changed files: {}", report.stats.mdformat_changed_files);
    println!(
        "  mdwright/mdformat different files: {}",
        report.stats.output_different_files
    );
    println!("  classified differences: {}", report.stats.classified_differences);
    println!("  unclassified differences: {}", report.stats.unclassified_differences);
    println!(
        "  ignored generated differences: {}",
        report.stats.ignored_generated_differences
    );
    println!(
        "  mdwright semantic drift failures: {}",
        report.stats.mdwright_semantic_drift_failures
    );
    println!(
        "  mdwright semantic parse errors: {}",
        report.stats.mdwright_semantic_parse_errors
    );
    println!("  mdformat semantic drift: {}", report.stats.mdformat_semantic_drift);
    println!(
        "  mdformat semantic parse errors: {}",
        report.stats.mdformat_semantic_parse_errors
    );
    println!(
        "  mdwright rendered HTML drift failures: {}",
        report.stats.mdwright_html_drift_failures
    );
    println!(
        "  mdwright rendered HTML parse errors: {}",
        report.stats.mdwright_html_parse_errors
    );
    println!("  mdformat rendered HTML drift: {}", report.stats.mdformat_html_drift);
    println!(
        "  mdformat rendered HTML parse errors: {}",
        report.stats.mdformat_html_parse_errors
    );
    println!(
        "  mdwright fmt-check failed: {}",
        yes_no(report.stats.mdwright_fmt_check_failed)
    );
    println!("  reports:");
    println!("    {}", output.join(REPORT_JSON).display());
    println!("    {}", output.join(REPORT_MD).display());
    if report.failures.is_empty() {
        println!("  result: pass");
    } else {
        println!("  result: fail");
        for failure in &report.failures {
            println!("    {failure}");
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn semantic_outcome_label(outcome: &SemanticOutcome) -> String {
    match outcome {
        SemanticOutcome::Equivalent => "equivalent".to_owned(),
        SemanticOutcome::Drift => "drift".to_owned(),
        SemanticOutcome::ParseError(err) => format!("parse error: {err}"),
        SemanticOutcome::NotChecked => "not checked".to_owned(),
    }
}

fn html_outcome_label(outcome: &HtmlOutcome) -> String {
    match outcome {
        HtmlOutcome::Equivalent => "equivalent".to_owned(),
        HtmlOutcome::Drift => "drift".to_owned(),
        HtmlOutcome::ParseError(err) => format!("parse error: {err}"),
        HtmlOutcome::NotChecked => "not checked".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn path_patterns_match_exact_and_prefix_rows() {
        assert!(path_matches("src/SUMMARY.md", "src/SUMMARY.md"));
        assert!(!path_matches("src/SUMMARY.md", "src/other.md"));
        assert!(path_matches("src/rules/**", "src/rules/index.md"));
        assert!(path_matches("src/rules/**", "src/rules/math/unbalanced-env.md"));
        assert!(!path_matches("src/rules/**", "src/reference/cli.md"));
    }

    #[test]
    fn classification_rows_match_by_corpus_and_path() {
        let rows = vec![
            ClassificationRow {
                corpus: "docs".to_owned(),
                path_pattern: "src/SUMMARY.md".to_owned(),
                construct: "list indentation".to_owned(),
                class: "style-option-mismatch".to_owned(),
                status: "intentional-divergence".to_owned(),
                owner: "formatter".to_owned(),
                resolution: "preserve source indentation".to_owned(),
            },
            ClassificationRow {
                corpus: "*".to_owned(),
                path_pattern: "src/rules/**".to_owned(),
                construct: "generated docs".to_owned(),
                class: "intentional-policy".to_owned(),
                status: "configured".to_owned(),
                owner: "docs".to_owned(),
                resolution: "generated output is excluded".to_owned(),
            },
        ];
        assert_eq!(
            find_classification(&rows, "docs", "src/SUMMARY.md")
                .expect("exact row matches")
                .construct,
            "list indentation"
        );
        assert_eq!(
            find_classification(&rows, "docs", "src/rules/index.md")
                .expect("prefix row matches")
                .status,
            "configured"
        );
        assert!(find_classification(&rows, "kan", "src/SUMMARY.md").is_none());
    }

    #[test]
    fn diff_summary_counts_changed_files_and_line_deltas() {
        let left = BTreeMap::from([
            ("a.md".to_owned(), "one\ntwo\nthree\n".to_owned()),
            ("b.md".to_owned(), "same\n".to_owned()),
        ]);
        let right = BTreeMap::from([
            ("a.md".to_owned(), "one\nthree\nfour\n".to_owned()),
            ("b.md".to_owned(), "same\n".to_owned()),
        ]);
        let summary = summarize_diff(&left, &right);
        assert_eq!(summary.changed_files, 1);
        assert_eq!(summary.deleted_lines, 1);
        assert_eq!(summary.inserted_lines, 1);
    }

    #[test]
    fn status_validation_rejects_unknown_words() {
        assert!(validate_status("intentional-divergence").is_ok());
        assert!(validate_status("open-bug").is_ok());
        assert!(validate_status("mystery").is_err());
    }

    #[test]
    fn rendered_html_comparison_ignores_softbreak_whitespace() {
        assert_eq!(
            normalize_rendered_html("<p>alpha\nbeta</p>\n"),
            normalize_rendered_html("<p>alpha beta</p>")
        );
    }
}
