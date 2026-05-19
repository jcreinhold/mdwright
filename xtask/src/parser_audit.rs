use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use mdwright_document::{
    Document, ExtensionOptions, GfmAutolinkPolicy, GfmOptions, NodeKind, ParseError, ParseOptions,
    render_html_with_options,
};
use regex::Regex;
use serde::Serialize;

use crate::gfm_spec::{SpecCase, parse_spec, spec_path};

const CMARK_GFM_REPO: &str = "https://github.com/github/cmark-gfm.git";
const CMARK_GFM_COMMIT: &str = "587a12bb54d95ac37241377e6ddc93ea0e45439b";
const CLASSIFICATION_PATH: &str = "docs/architecture/parser-backend-audit.md";
const REPORT_JSON: &str = "parser-audit.json";
const REPORT_MD: &str = "parser-audit.md";
const LINK_REF_TAB_REPRO: &str = "- [n]:Z\r\n\t\t";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseSet {
    GfmSpec,
    Corpus,
    All,
}

#[derive(Clone, Debug)]
pub struct ParserAuditOptions {
    pub case_set: CaseSet,
    pub output: PathBuf,
    pub ensure_tools: bool,
    pub include_comrak: bool,
    pub cmark_gfm_bin: Option<PathBuf>,
    pub corpus_root: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
struct AuditReport {
    cmark_gfm_bin: String,
    cmark_gfm_commit: String,
    include_comrak: bool,
    stats: AuditStats,
    differences: Vec<DifferenceReport>,
    failures: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct AuditStats {
    cases: usize,
    gfm_spec_cases: usize,
    corpus_cases: usize,
    mdwright_parse_errors: usize,
    cmark_failures: usize,
    cmark_expected_mismatches: usize,
    pulldown_html_mismatches: usize,
    comrak_html_mismatches: usize,
    sourcepos_risks: usize,
    sourcepos_checked: usize,
    sourcepos_differences: usize,
    sourcepos_unclassified: usize,
    sourcepos_mitigations: usize,
    classified_differences: usize,
    unclassified_differences: usize,
    fixed_rows_still_observed: usize,
    mitigation_rows_observed: usize,
}

#[derive(Clone, Debug)]
struct AuditCase {
    case_set: String,
    key: String,
    label: String,
    classes: Vec<String>,
    cmark_extensions: Vec<String>,
    source: String,
    expected_html: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct DifferenceReport {
    case_set: String,
    key: String,
    label: String,
    observed: String,
    status: Option<String>,
    owner: Option<String>,
    resolution: Option<String>,
    cmark_html: Option<String>,
    pulldown_html: Option<String>,
    comrak_html: Option<String>,
    sourcepos: SourceposSummary,
}

#[derive(Clone, Debug, Default, Serialize)]
struct SourceposSummary {
    cmark_sourcepos_attrs: usize,
    comrak_sourcepos_attrs: Option<usize>,
    mdwright_structural_facts: Option<usize>,
    checked: usize,
    differences: usize,
    risks: Vec<String>,
}

#[derive(Clone, Debug)]
struct SourceposRisk {
    observed: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassificationRow {
    case_set: String,
    key_pattern: String,
    observed: String,
    status: String,
    owner: String,
    resolution: String,
}

#[derive(Clone, Debug)]
struct Rendered {
    html: String,
    sourcepos_html: String,
}

pub fn run(workspace: &Path, opts: ParserAuditOptions) -> Result<bool> {
    let cmark = resolve_cmark_gfm(workspace, opts.cmark_gfm_bin.as_deref(), opts.ensure_tools)?;
    validate_cmark_gfm(&cmark)?;
    let output = if opts.output.is_absolute() {
        opts.output.clone()
    } else {
        workspace.join(&opts.output)
    };
    let classifications = load_classifications(&workspace.join(CLASSIFICATION_PATH))?;
    let mut stats = AuditStats::default();
    let mut failures = Vec::new();
    let mut differences = Vec::new();

    let cases = collect_cases(workspace, &opts)?;
    stats.cases = cases.len();
    stats.gfm_spec_cases = cases.iter().filter(|case| case.case_set == "gfm-spec").count();
    stats.corpus_cases = cases.iter().filter(|case| case.case_set == "corpus").count();

    for case in cases {
        let cmark_rendered = match render_with_cmark(&cmark, &case) {
            Ok(rendered) => rendered,
            Err(err) => {
                stats.cmark_failures = stats.cmark_failures.saturating_add(1);
                failures.push(format!("cmark-gfm failed for {} {}: {err:#}", case.case_set, case.key));
                continue;
            }
        };
        if let Some(expected) = &case.expected_html
            && normalize_spec_html(expected) != normalize_spec_html(&cmark_rendered.html)
        {
            stats.cmark_expected_mismatches = stats.cmark_expected_mismatches.saturating_add(1);
            if !case.classes.iter().any(|class| class == "disabled") {
                failures.push(format!(
                    "cmark-gfm rendered HTML differs from vendored expected HTML for {} {}",
                    case.case_set, case.key
                ));
            }
        }

        let comrak_rendered = opts.include_comrak.then(|| render_with_comrak(&case.source));
        let comrak_sourcepos = comrak_rendered
            .as_ref()
            .map(|rendered| rendered.sourcepos_html.as_str());
        let sourcepos = sourcepos_analysis(&case, &cmark_rendered.sourcepos_html, comrak_sourcepos);
        stats.sourcepos_checked = stats.sourcepos_checked.saturating_add(sourcepos.summary.checked);

        let pulldown_html = match render_html_with_options(&case.source, audit_parse_options(&case)) {
            Ok(html) => Some(html),
            Err(err) => {
                stats.mdwright_parse_errors = stats.mdwright_parse_errors.saturating_add(1);
                let observed = classify_parse_error(&case, &err);
                record_difference(
                    &mut stats,
                    &mut failures,
                    &mut differences,
                    &classifications,
                    DifferenceInput {
                        case: &case,
                        observed: &observed,
                        pulldown_html: None,
                        cmark_html: Some(cmark_rendered.html.clone()),
                        comrak_html: None,
                        sourcepos: sourcepos.summary.clone(),
                    },
                );
                continue;
            }
        };
        let pulldown_html = pulldown_html.expect("parse errors continue");
        let oracle_html = case.expected_html.as_deref().unwrap_or(&cmark_rendered.html);
        let html_mismatched = normalize_spec_html(oracle_html) != normalize_spec_html(&pulldown_html);
        if html_mismatched {
            let observed = classify_html_mismatch(&case, oracle_html, &pulldown_html);
            stats.pulldown_html_mismatches = stats.pulldown_html_mismatches.saturating_add(1);
            let comrak_html = comrak_rendered.as_ref().map(|rendered| rendered.html.clone());
            record_difference(
                &mut stats,
                &mut failures,
                &mut differences,
                &classifications,
                DifferenceInput {
                    case: &case,
                    observed: &observed,
                    pulldown_html: Some(pulldown_html.clone()),
                    cmark_html: Some(cmark_rendered.html.clone()),
                    comrak_html,
                    sourcepos: sourcepos.summary.clone(),
                },
            );
        } else if let Some(comrak) = &comrak_rendered
            && normalize_spec_html(oracle_html) != normalize_spec_html(&comrak.html)
        {
            stats.comrak_html_mismatches = stats.comrak_html_mismatches.saturating_add(1);
        }

        if !sourcepos.risks.is_empty() {
            stats.sourcepos_risks = stats.sourcepos_risks.saturating_add(1);
            stats.sourcepos_differences = stats.sourcepos_differences.saturating_add(sourcepos.risks.len());
            for risk in sourcepos.risks {
                let before_unclassified = stats.unclassified_differences;
                let before_mitigation = stats.mitigation_rows_observed;
                record_difference(
                    &mut stats,
                    &mut failures,
                    &mut differences,
                    &classifications,
                    DifferenceInput {
                        case: &case,
                        observed: &risk.observed,
                        pulldown_html: Some(pulldown_html.clone()),
                        cmark_html: Some(cmark_rendered.html.clone()),
                        comrak_html: comrak_rendered.as_ref().map(|rendered| rendered.html.clone()),
                        sourcepos: sourcepos.summary.clone(),
                    },
                );
                if stats.unclassified_differences > before_unclassified {
                    stats.sourcepos_unclassified = stats.sourcepos_unclassified.saturating_add(1);
                }
                if stats.mitigation_rows_observed > before_mitigation {
                    stats.sourcepos_mitigations = stats.sourcepos_mitigations.saturating_add(1);
                }
            }
        }
    }

    let report = AuditReport {
        cmark_gfm_bin: cmark.display().to_string(),
        cmark_gfm_commit: CMARK_GFM_COMMIT.to_owned(),
        include_comrak: opts.include_comrak,
        stats,
        differences,
        failures,
    };
    write_reports(&output, &report)?;
    print_summary(&output, &report);
    Ok(report.failures.is_empty())
}

fn collect_cases(workspace: &Path, opts: &ParserAuditOptions) -> Result<Vec<AuditCase>> {
    let mut cases = Vec::new();
    if matches!(opts.case_set, CaseSet::GfmSpec | CaseSet::All) {
        let text = fs::read_to_string(spec_path(workspace)).context("read vendored GFM spec")?;
        cases.extend(parse_spec(&text).into_iter().map(spec_case));
        cases.push(AuditCase {
            case_set: "operational".to_owned(),
            key: "known-pulldown-link-ref-tab-panic".to_owned(),
            label: "known pulldown link-reference tab panic".to_owned(),
            classes: Vec::new(),
            cmark_extensions: Vec::new(),
            source: LINK_REF_TAB_REPRO.to_owned(),
            expected_html: None,
        });
    }
    if matches!(opts.case_set, CaseSet::Corpus | CaseSet::All) {
        cases.extend(corpus_cases(workspace, opts.corpus_root.as_deref())?);
    }
    Ok(cases)
}

fn spec_case(case: SpecCase) -> AuditCase {
    let cmark_extensions = cmark_extensions_for_spec_case(&case);
    AuditCase {
        case_set: "gfm-spec".to_owned(),
        key: format!("case-{}", case.number),
        label: case.section,
        classes: case.classes,
        cmark_extensions,
        source: case.source,
        expected_html: Some(case.expected_html),
    }
}

fn cmark_extensions_for_spec_case(case: &SpecCase) -> Vec<String> {
    let mut extensions = Vec::new();
    for class in &case.classes {
        match class.as_str() {
            "autolink" | "strikethrough" | "table" | "tagfilter" => extensions.push(class.clone()),
            "disabled" => extensions.push("tasklist".to_owned()),
            _ => {}
        }
    }
    if case.section == "Task list items (extension)" && !extensions.iter().any(|extension| extension == "tasklist") {
        extensions.push("tasklist".to_owned());
    }
    extensions
}

fn corpus_cases(workspace: &Path, release_corpus_root: Option<&Path>) -> Result<Vec<AuditCase>> {
    let mut roots = vec![
        ("mdwright-docs".to_owned(), workspace.join("docs")),
        ("external".to_owned(), workspace.join("crates/mdwright/tests/external")),
    ];
    let kan_docs = PathBuf::from("/Users/jcreinhold/Code/kan/docs");
    if kan_docs.exists() {
        roots.push(("kan-docs".to_owned(), kan_docs));
    }
    let release_root = release_corpus_root
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("MDWRIGHT_CORPUS_ROOT").map(PathBuf::from))
        .or_else(|| {
            let fallback = PathBuf::from("/Users/jcreinhold/Code/kan");
            fallback.exists().then_some(fallback)
        });
    if let Some(root) = release_root {
        roots.push(("release-corpus".to_owned(), root));
    }

    let mut cases = Vec::new();
    let mut seen = BTreeSet::new();
    for (name, root) in roots {
        if !root.exists() {
            continue;
        }
        for path in collect_markdown_files(&root)? {
            let rel = relative_path(&root, &path)?;
            let key = format!("{name}:{rel}");
            if !seen.insert(key.clone()) {
                continue;
            }
            let source = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            cases.push(AuditCase {
                case_set: "corpus".to_owned(),
                key,
                label: rel,
                classes: Vec::new(),
                cmark_extensions: corpus_cmark_extensions(),
                source,
                expected_html: None,
            });
        }
    }
    Ok(cases)
}

fn collect_markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_markdown_files_inner(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_markdown_files_inner(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if root.is_file() {
        if root.extension().and_then(|ext| ext.to_str()) == Some("md") {
            out.push(root.to_path_buf());
        }
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry.with_context(|| format!("read entry under {}", root.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name == "book" || name == "target" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_markdown_files_inner(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)
        .with_context(|| format!("strip {} from {}", root.display(), path.display()))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn resolve_cmark_gfm(workspace: &Path, override_bin: Option<&Path>, ensure_tools: bool) -> Result<PathBuf> {
    if let Some(bin) = override_bin {
        return bin
            .canonicalize()
            .with_context(|| format!("canonicalize cmark-gfm binary {}", bin.display()));
    }
    let install_bin = workspace
        .join("target/mdwright/tools/cmark-gfm")
        .join(CMARK_GFM_COMMIT)
        .join("install/bin/cmark-gfm");
    if install_bin.is_file() {
        return Ok(install_bin);
    }
    if !ensure_tools {
        bail!(
            "no pinned cmark-gfm binary at {}; pass --ensure-tools or --cmark-gfm-bin",
            install_bin.display()
        );
    }
    build_cmark_gfm(workspace, &install_bin)?;
    Ok(install_bin)
}

fn build_cmark_gfm(workspace: &Path, install_bin: &Path) -> Result<()> {
    let tool_root = workspace.join("target/mdwright/tools/cmark-gfm").join(CMARK_GFM_COMMIT);
    let src = tool_root.join("src");
    let build = tool_root.join("build");
    let install = tool_root.join("install");
    fs::create_dir_all(&tool_root).with_context(|| format!("create {}", tool_root.display()))?;
    if !src.join(".git").is_dir() {
        run_command(Command::new("git").args([
            "clone",
            "--recursive",
            CMARK_GFM_REPO,
            src.to_string_lossy().as_ref(),
        ]))?;
    }
    run_command(
        Command::new("git")
            .current_dir(&src)
            .args(["fetch", "--tags", "origin"]),
    )?;
    run_command(
        Command::new("git")
            .current_dir(&src)
            .args(["checkout", CMARK_GFM_COMMIT]),
    )?;
    run_command(
        Command::new("git")
            .current_dir(&src)
            .args(["submodule", "update", "--init", "--recursive"]),
    )?;
    run_command(Command::new("cmake").args([
        "-S",
        src.to_string_lossy().as_ref(),
        "-B",
        build.to_string_lossy().as_ref(),
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
        &format!("-DCMAKE_INSTALL_PREFIX={}", install.display()),
    ]))?;
    run_command(Command::new("cmake").args([
        "--build",
        build.to_string_lossy().as_ref(),
        "--target",
        "install",
        "--config",
        "Release",
    ]))?;
    if !install_bin.is_file() {
        bail!("cmark-gfm build did not produce {}", install_bin.display());
    }
    Ok(())
}

fn corpus_cmark_extensions() -> Vec<String> {
    ["autolink", "strikethrough", "table", "tagfilter", "tasklist"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn audit_parse_options(case: &AuditCase) -> ParseOptions {
    let autolinks = if case.cmark_extensions.iter().any(|extension| extension == "autolink") {
        GfmAutolinkPolicy::UrlsAndEmails
    } else {
        GfmAutolinkPolicy::Disabled
    };
    let tagfilter = case.cmark_extensions.iter().any(|extension| extension == "tagfilter");
    ParseOptions::default().with_extensions(ExtensionOptions {
        gfm: GfmOptions { autolinks, tagfilter },
        ..ExtensionOptions::default()
    })
}

fn render_with_cmark(bin: &Path, case: &AuditCase) -> Result<Rendered> {
    let html = run_cmark(bin, &case.source, &case.cmark_extensions, false)?;
    let sourcepos_html = run_cmark(bin, &case.source, &case.cmark_extensions, true)?;
    Ok(Rendered { html, sourcepos_html })
}

fn validate_cmark_gfm(bin: &Path) -> Result<()> {
    let case = AuditCase {
        case_set: "smoke".to_owned(),
        key: "smoke".to_owned(),
        label: "smoke".to_owned(),
        classes: Vec::new(),
        cmark_extensions: Vec::new(),
        source: "hello\n".to_owned(),
        expected_html: None,
    };
    let rendered =
        render_with_cmark(bin, &case).with_context(|| format!("validate cmark-gfm binary {}", bin.display()))?;
    if normalize_spec_html(&rendered.html) != "<p>hello</p>\n" {
        bail!("{} did not render a basic smoke case as expected", bin.display());
    }
    Ok(())
}

fn run_cmark(bin: &Path, source: &str, extensions: &[String], sourcepos: bool) -> Result<String> {
    let mut command = Command::new(bin);
    configure_cmark_runtime_env(&mut command, bin);
    command.arg("--unsafe");
    for extension in extensions {
        command.args(["--extension", extension]);
    }
    if sourcepos {
        command.arg("--sourcepos");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {}", bin.display()))?;
    child
        .stdin
        .as_mut()
        .context("open cmark-gfm stdin")?
        .write_all(source.as_bytes())
        .context("write cmark-gfm stdin")?;
    let output = child.wait_with_output().context("wait for cmark-gfm")?;
    if !output.status.success() {
        bail!(
            "{} exited with {}\nstderr:\n{}",
            bin.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).context("cmark-gfm emitted non-UTF-8")
}

fn configure_cmark_runtime_env(command: &mut Command, bin: &Path) {
    let Some(prefix) = bin.parent().and_then(Path::parent) else {
        return;
    };
    let lib = prefix.join("lib");
    if !lib.is_dir() {
        return;
    }
    prepend_env_path(command, "DYLD_LIBRARY_PATH", &lib);
    prepend_env_path(command, "DYLD_FALLBACK_LIBRARY_PATH", &lib);
    prepend_env_path(command, "LD_LIBRARY_PATH", &lib);
}

fn prepend_env_path(command: &mut Command, key: &str, path: &Path) {
    let mut value = path.as_os_str().to_os_string();
    if let Some(old) = std::env::var_os(key) {
        value.push(":");
        value.push(old);
    }
    command.env(key, value);
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
    )
}

fn render_with_comrak(source: &str) -> Rendered {
    let mut options = comrak_options(false);
    let html = comrak::markdown_to_html(source, &options);
    options.render.sourcepos = true;
    let sourcepos_html = comrak::markdown_to_html(source, &options);
    Rendered { html, sourcepos_html }
}

fn comrak_options(sourcepos: bool) -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tagfilter = true;
    options.extension.tasklist = true;
    options.render.r#unsafe = true;
    options.render.sourcepos = sourcepos;
    options
}

fn normalize_spec_html(html: &str) -> String {
    let mut normalized = html.replace("\r\n", "\n").replace('\r', "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn classify_parse_error(case: &AuditCase, _err: &ParseError) -> String {
    if case.key == "known-pulldown-link-ref-tab-panic" {
        "upstream-panic".to_owned()
    } else {
        "needs-mdwright-mitigation".to_owned()
    }
}

fn classify_html_mismatch(case: &AuditCase, oracle_html: &str, pulldown_html: &str) -> String {
    if html_equivalent_after_quote_unescape(oracle_html, pulldown_html) {
        "pulldown-html-mismatch:quote-escaping".to_owned()
    } else if oracle_html.contains("<table") || pulldown_html.contains("<table") {
        "pulldown-html-mismatch:table-rendering".to_owned()
    } else if (case.classes.iter().any(|class| matches!(class.as_str(), "autolink"))
        || looks_like_bare_url(&case.source))
        && autolink_html_mismatch(oracle_html, pulldown_html)
    {
        "pulldown-html-mismatch:gfm-autolink".to_owned()
    } else if case.classes.iter().any(|class| matches!(class.as_str(), "tagfilter")) {
        "pulldown-html-mismatch:gfm-tagfilter".to_owned()
    } else if pulldown_html.contains("<dl>") && case.source.contains(":::") {
        "extension-gap:myst-definition-list".to_owned()
    } else if oracle_html.contains("type=\"checkbox\"") || pulldown_html.contains("type=\"checkbox\"") {
        "pulldown-html-mismatch:tasklist-rendering".to_owned()
    } else if case.label.contains("Emphasis and strong emphasis") {
        "pulldown-html-mismatch:emphasis-resolution".to_owned()
    } else if oracle_html.contains('<') && pulldown_html.contains('<') && case.source.contains('<') {
        "pulldown-html-mismatch:html-block-rendering".to_owned()
    } else {
        "pulldown-html-mismatch".to_owned()
    }
}

fn html_equivalent_after_quote_unescape(left: &str, right: &str) -> bool {
    normalize_spec_html(left).replace("&quot;", "\"") == normalize_spec_html(right).replace("&quot;", "\"")
}

fn autolink_html_mismatch(left: &str, right: &str) -> bool {
    autolink_href_count(left) != autolink_href_count(right)
}

fn autolink_href_count(html: &str) -> usize {
    ["href=\"http://", "href=\"https://", "href=\"ftp://", "href=\"mailto:"]
        .into_iter()
        .map(|needle| html.match_indices(needle).count())
        .sum()
}

fn looks_like_bare_url(source: &str) -> bool {
    source.contains("http://") || source.contains("https://") || source.contains("ftp://") || source.contains("www.")
}

#[derive(Clone, Debug)]
struct SourceposAnalysis {
    summary: SourceposSummary,
    risks: Vec<SourceposRisk>,
}

#[derive(Clone, Debug)]
struct SourceposEnvelope {
    kind: &'static str,
    range: std::ops::Range<usize>,
}

fn sourcepos_analysis(
    case: &AuditCase,
    cmark_sourcepos_html: &str,
    comrak_sourcepos_html: Option<&str>,
) -> SourceposAnalysis {
    let doc = Document::parse(&case.source).ok();
    let mdwright_structural_facts = doc.as_ref().map(|doc| {
        doc.headings().len()
            + doc.list_groups().len()
            + doc.code_blocks().len()
            + doc.html_blocks().len()
            + doc.inline_codes().len()
            + doc.link_defs().len()
            + doc.autolinks().len()
            + usize::from(doc.frontmatter().is_some())
    });
    let envelopes = cmark_sourcepos_envelopes(&case.source, cmark_sourcepos_html);
    let risks = doc
        .as_ref()
        .map_or_else(Vec::new, |doc| sourcepos_risks(doc, &envelopes));
    let summary = SourceposSummary {
        cmark_sourcepos_attrs: count_sourcepos_attrs(cmark_sourcepos_html),
        comrak_sourcepos_attrs: comrak_sourcepos_html.map(count_sourcepos_attrs),
        mdwright_structural_facts,
        checked: envelopes.len(),
        differences: risks.len(),
        risks: risks.iter().map(|risk| risk.observed.clone()).collect(),
    };
    SourceposAnalysis { summary, risks }
}

#[allow(
    clippy::expect_used,
    reason = "static sourcepos regex is covered by parser-audit tests"
)]
fn sourcepos_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"<(?P<tag>[A-Za-z][A-Za-z0-9]*)(?:\s+[^>]*)?\sdata-sourcepos="(?P<sl>\d+):(?P<sc>\d+)-(?P<el>\d+):(?P<ec>\d+)""#)
            .expect("sourcepos regex compiles")
    })
}

fn cmark_sourcepos_envelopes(source: &str, html: &str) -> Vec<SourceposEnvelope> {
    let index = mdwright_document::LineIndex::new(source);
    sourcepos_regex()
        .captures_iter(html)
        .filter_map(|caps| {
            let tag = caps.name("tag")?.as_str();
            let kind = sourcepos_kind_for_tag(tag)?;
            let start_line = parse_usize(caps.name("sl")?.as_str())?;
            let start_col = parse_usize(caps.name("sc")?.as_str())?;
            let end_line = parse_usize(caps.name("el")?.as_str())?;
            let end_col = parse_usize(caps.name("ec")?.as_str())?;
            let start =
                index.byte_of_position_0based(source, start_line.saturating_sub(1), start_col.saturating_sub(1))?;
            let end = index.byte_of_position_0based(source, end_line.saturating_sub(1), end_col)?;
            Some(SourceposEnvelope {
                kind,
                range: start..end,
            })
        })
        .collect()
}

fn parse_usize(s: &str) -> Option<usize> {
    s.parse().ok()
}

fn sourcepos_kind_for_tag(tag: &str) -> Option<&'static str> {
    match tag {
        "p" => Some("paragraph"),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some("heading"),
        "ul" | "ol" => Some("list"),
        "li" => Some("list-item"),
        "blockquote" => Some("blockquote"),
        "pre" => Some("code-block"),
        "table" => Some("table"),
        _ => None,
    }
}

fn sourcepos_risks(doc: &Document, envelopes: &[SourceposEnvelope]) -> Vec<SourceposRisk> {
    let facts = mdwright_sourcepos_facts(doc);
    let frontmatter = doc.frontmatter().map(|frontmatter| frontmatter.slice.raw_range.clone());
    envelopes
        .iter()
        .filter(|envelope| {
            frontmatter
                .as_ref()
                .is_none_or(|range| !ranges_overlap(range, &envelope.range))
        })
        .filter(|envelope| {
            !facts
                .iter()
                .any(|fact| fact.kind == envelope.kind && ranges_overlap(&fact.range, &envelope.range))
        })
        .map(|envelope| SourceposRisk {
            observed: format!("sourcepos-risk:{}", envelope.kind),
        })
        .collect()
}

fn mdwright_sourcepos_facts(doc: &Document) -> Vec<SourceposEnvelope> {
    let mut facts = Vec::new();
    let tree = doc.tree();
    for id in tree.descendants(tree.root()) {
        let Some(node) = tree.node(id) else { continue };
        let kind = match node.kind {
            NodeKind::Paragraph => Some("paragraph"),
            NodeKind::Heading { .. } => Some("heading"),
            NodeKind::List { .. } => Some("list"),
            NodeKind::Item { .. } => Some("list-item"),
            NodeKind::BlockQuote => Some("blockquote"),
            NodeKind::CodeBlock { .. } => Some("code-block"),
            NodeKind::Table { .. } => Some("table"),
            _ => None,
        };
        if let Some(kind) = kind {
            facts.push(SourceposEnvelope {
                kind,
                range: node.raw_range.clone(),
            });
        }
    }
    facts
}

fn ranges_overlap(left: &std::ops::Range<usize>, right: &std::ops::Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn count_sourcepos_attrs(html: &str) -> usize {
    html.match_indices("data-sourcepos=").count()
}

struct DifferenceInput<'a> {
    case: &'a AuditCase,
    observed: &'a str,
    pulldown_html: Option<String>,
    cmark_html: Option<String>,
    comrak_html: Option<String>,
    sourcepos: SourceposSummary,
}

fn record_difference(
    stats: &mut AuditStats,
    failures: &mut Vec<String>,
    differences: &mut Vec<DifferenceReport>,
    rows: &[ClassificationRow],
    input: DifferenceInput<'_>,
) {
    let case = input.case;
    let observed = input.observed;
    let row = find_classification(rows, case, observed);
    let mut status = None;
    let mut owner = None;
    let mut resolution = None;
    if let Some(row) = row {
        stats.classified_differences = stats.classified_differences.saturating_add(1);
        status = Some(row.status.clone());
        owner = Some(row.owner.clone());
        resolution = Some(row.resolution.clone());
        if row.status == "fixed" {
            stats.fixed_rows_still_observed = stats.fixed_rows_still_observed.saturating_add(1);
            failures.push(format!(
                "parser difference marked fixed still observed: {} {} ({observed})",
                case.case_set, case.key
            ));
        }
        if row.status == "needs-mdwright-mitigation" {
            stats.mitigation_rows_observed = stats.mitigation_rows_observed.saturating_add(1);
            failures.push(format!(
                "parser difference needs mdwright mitigation: {} {} ({observed})",
                case.case_set, case.key
            ));
        }
    } else {
        stats.unclassified_differences = stats.unclassified_differences.saturating_add(1);
        failures.push(format!(
            "unclassified parser difference: {} {} ({observed})",
            case.case_set, case.key
        ));
    }
    differences.push(DifferenceReport {
        case_set: case.case_set.clone(),
        key: case.key.clone(),
        label: case.label.clone(),
        observed: observed.to_owned(),
        status,
        owner,
        resolution,
        cmark_html: input.cmark_html,
        pulldown_html: input.pulldown_html,
        comrak_html: input.comrak_html,
        sourcepos: input.sourcepos,
    });
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
        if cells.len() < 6 || cells[0] == "Case Set" || cells.iter().all(|cell| cell.chars().all(|c| c == '-')) {
            continue;
        }
        validate_status(&cells[3]).with_context(|| format!("invalid parser-audit status for {}", cells[1]))?;
        rows.push(ClassificationRow {
            case_set: cells[0].clone(),
            key_pattern: cells[1].clone(),
            observed: cells[2].clone(),
            status: cells[3].clone(),
            owner: cells[4].clone(),
            resolution: cells[5].clone(),
        });
    }
    Ok(rows)
}

fn validate_status(status: &str) -> Result<()> {
    if matches!(
        status,
        "pulldown-html-mismatch"
            | "mdwright-policy"
            | "extension-gap"
            | "sourcepos-risk"
            | "event-only"
            | "upstream-panic"
            | "needs-mdwright-mitigation"
            | "fixed"
    ) {
        Ok(())
    } else {
        bail!("unknown parser-audit status `{status}`")
    }
}

fn find_classification<'a>(
    rows: &'a [ClassificationRow],
    case: &AuditCase,
    observed: &str,
) -> Option<&'a ClassificationRow> {
    rows.iter().find(|row| {
        pattern_matches(&row.case_set, &case.case_set)
            && (pattern_matches(&row.key_pattern, &case.key) || pattern_matches(&row.key_pattern, &case.label))
            && pattern_matches(&row.observed, observed)
    })
}

fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern.contains(',') {
        return pattern.split(',').any(|part| pattern_matches(part.trim(), value));
    }
    if pattern == "*" || pattern == "**" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return value == prefix || value.starts_with(&format!("{prefix}/"));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    pattern == value
}

fn write_reports(output: &Path, report: &AuditReport) -> Result<()> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    let json_path = output.join(REPORT_JSON);
    let md_path = output.join(REPORT_MD);
    fs::write(
        &json_path,
        serde_json::to_string_pretty(report).context("serialize parser audit report")?,
    )
    .with_context(|| format!("write {}", json_path.display()))?;
    fs::write(&md_path, markdown_report(report)).with_context(|| format!("write {}", md_path.display()))?;
    Ok(())
}

fn markdown_report(report: &AuditReport) -> String {
    let mut out = String::new();
    out.push_str("# Parser backend audit report\n\n");
    out.push_str(&format!("- cmark-gfm binary: `{}`\n", report.cmark_gfm_bin));
    out.push_str(&format!("- cmark-gfm commit: `{}`\n", report.cmark_gfm_commit));
    out.push_str(&format!("- comrak diagnostics: `{}`\n", yes_no(report.include_comrak)));
    out.push_str(&format!("- cases: `{}`\n", report.stats.cases));
    out.push_str(&format!("- GFM spec cases: `{}`\n", report.stats.gfm_spec_cases));
    out.push_str(&format!("- corpus cases: `{}`\n", report.stats.corpus_cases));
    out.push_str(&format!(
        "- pulldown HTML mismatches: `{}`\n",
        report.stats.pulldown_html_mismatches
    ));
    out.push_str(&format!(
        "- mdwright parse errors: `{}`\n",
        report.stats.mdwright_parse_errors
    ));
    out.push_str(&format!(
        "- unclassified differences: `{}`\n",
        report.stats.unclassified_differences
    ));
    out.push_str(&format!("- sourcepos checked: `{}`\n", report.stats.sourcepos_checked));
    out.push_str(&format!(
        "- sourcepos differences: `{}`\n",
        report.stats.sourcepos_differences
    ));
    out.push_str("\n## Differences\n\n");
    out.push_str("| Case Set | Key | Observed | Status | Owner | Resolution |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for diff in &report.differences {
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} |\n",
            diff.case_set,
            diff.key,
            diff.observed,
            diff.status.as_deref().unwrap_or("unclassified"),
            diff.owner.as_deref().unwrap_or(""),
            diff.resolution.as_deref().unwrap_or("")
        ));
    }
    if !report.failures.is_empty() {
        out.push_str("\n## Failures\n\n");
        for failure in &report.failures {
            out.push_str(&format!("- {failure}\n"));
        }
    }
    out
}

fn print_summary(output: &Path, report: &AuditReport) {
    println!("parser audit summary");
    println!("  cases: {}", report.stats.cases);
    println!("  gfm spec cases: {}", report.stats.gfm_spec_cases);
    println!("  corpus cases: {}", report.stats.corpus_cases);
    println!("  mdwright parse errors: {}", report.stats.mdwright_parse_errors);
    println!("  cmark failures: {}", report.stats.cmark_failures);
    println!(
        "  cmark expected mismatches: {}",
        report.stats.cmark_expected_mismatches
    );
    println!("  pulldown HTML mismatches: {}", report.stats.pulldown_html_mismatches);
    println!("  comrak HTML mismatches: {}", report.stats.comrak_html_mismatches);
    println!("  sourcepos risks: {}", report.stats.sourcepos_risks);
    println!("  sourcepos checked: {}", report.stats.sourcepos_checked);
    println!("  sourcepos differences: {}", report.stats.sourcepos_differences);
    println!("  sourcepos unclassified: {}", report.stats.sourcepos_unclassified);
    println!("  sourcepos mitigations: {}", report.stats.sourcepos_mitigations);
    println!("  classified differences: {}", report.stats.classified_differences);
    println!("  unclassified differences: {}", report.stats.unclassified_differences);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_rows_match_by_case_key_and_observed_class() {
        let rows = vec![
            ClassificationRow {
                case_set: "gfm-spec".to_owned(),
                key_pattern: "case-621, case-622, case-623".to_owned(),
                observed: "mdwright-policy".to_owned(),
                status: "mdwright-policy".to_owned(),
                owner: "document".to_owned(),
                resolution: "autolinks disabled".to_owned(),
            },
            ClassificationRow {
                case_set: "gfm-spec".to_owned(),
                key_pattern: "Autolinks*".to_owned(),
                observed: "mdwright-policy".to_owned(),
                status: "mdwright-policy".to_owned(),
                owner: "document".to_owned(),
                resolution: "section label match".to_owned(),
            },
        ];
        let case = test_case("gfm-spec", "case-622", "Autolinks (extension)");
        let row = find_classification(&rows, &case, "mdwright-policy").expect("row matches alternative case key");
        assert_eq!(row.status, "mdwright-policy");
        let section_case = test_case("gfm-spec", "case-629", "Autolinks (extension)");
        assert!(find_classification(&rows, &section_case, "mdwright-policy").is_some());
        let unmatched = test_case("gfm-spec", "case-1", "Tabs");
        assert!(find_classification(&rows, &unmatched, "pulldown-html-mismatch").is_none());
    }

    #[test]
    fn fixed_rows_are_valid_but_remain_failing_when_observed() {
        assert!(validate_status("fixed").is_ok());
        let rows = vec![ClassificationRow {
            case_set: "*".to_owned(),
            key_pattern: "*".to_owned(),
            observed: "pulldown-html-mismatch".to_owned(),
            status: "fixed".to_owned(),
            owner: "document".to_owned(),
            resolution: "should be gone".to_owned(),
        }];
        let case = AuditCase {
            case_set: "gfm-spec".to_owned(),
            key: "case-1".to_owned(),
            label: String::new(),
            classes: Vec::new(),
            cmark_extensions: Vec::new(),
            source: String::new(),
            expected_html: None,
        };
        let mut stats = AuditStats::default();
        let mut failures = Vec::new();
        let mut differences = Vec::new();
        record_difference(
            &mut stats,
            &mut failures,
            &mut differences,
            &rows,
            DifferenceInput {
                case: &case,
                observed: "pulldown-html-mismatch",
                pulldown_html: None,
                cmark_html: None,
                comrak_html: None,
                sourcepos: SourceposSummary::default(),
            },
        );
        assert_eq!(stats.fixed_rows_still_observed, 1);
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn event_only_is_not_an_html_failure_class() {
        let case = AuditCase {
            case_set: "gfm-spec".to_owned(),
            key: "case-5".to_owned(),
            label: String::new(),
            classes: Vec::new(),
            cmark_extensions: Vec::new(),
            source: "hello\n".to_owned(),
            expected_html: Some("<p>hello</p>\n".to_owned()),
        };
        assert_eq!(
            classify_html_mismatch(&case, "<p>expected</p>\n", "<p>actual</p>\n"),
            "pulldown-html-mismatch"
        );
    }

    #[test]
    fn sourcepos_envelopes_map_cmark_line_columns_to_bytes() {
        let source = "# Head\n\nParagraph\n";
        let html = r#"<h1 data-sourcepos="1:1-1:6">Head</h1>
<p data-sourcepos="3:1-3:9">Paragraph</p>
"#;
        let envelopes = cmark_sourcepos_envelopes(source, html);
        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].kind, "heading");
        assert_eq!(envelopes[0].range, 0..6);
        assert_eq!(envelopes[1].kind, "paragraph");
        assert_eq!(envelopes[1].range, 8..17);
    }

    #[test]
    fn sourcepos_risks_are_recorded_as_classified_differences() {
        let rows = vec![ClassificationRow {
            case_set: "*".to_owned(),
            key_pattern: "*".to_owned(),
            observed: "sourcepos-risk:*".to_owned(),
            status: "sourcepos-risk".to_owned(),
            owner: "document".to_owned(),
            resolution: "synthetic sourcepos risk classification".to_owned(),
        }];
        let case = test_case("gfm-spec", "case-1", "Tabs");
        let mut stats = AuditStats::default();
        let mut failures = Vec::new();
        let mut differences = Vec::new();
        record_difference(
            &mut stats,
            &mut failures,
            &mut differences,
            &rows,
            DifferenceInput {
                case: &case,
                observed: "sourcepos-risk:heading",
                pulldown_html: None,
                cmark_html: None,
                comrak_html: None,
                sourcepos: SourceposSummary::default(),
            },
        );
        assert_eq!(stats.classified_differences, 1);
        assert!(failures.is_empty());
    }

    #[test]
    fn unclassified_sourcepos_risks_fail() {
        let case = test_case("gfm-spec", "case-1", "Tabs");
        let mut stats = AuditStats::default();
        let mut failures = Vec::new();
        let mut differences = Vec::new();
        record_difference(
            &mut stats,
            &mut failures,
            &mut differences,
            &[],
            DifferenceInput {
                case: &case,
                observed: "sourcepos-risk:heading",
                pulldown_html: None,
                cmark_html: None,
                comrak_html: None,
                sourcepos: SourceposSummary::default(),
            },
        );
        assert_eq!(stats.unclassified_differences, 1);
        assert_eq!(failures.len(), 1);
    }

    #[test]
    fn known_pulldown_panic_is_classified_as_upstream() {
        let case = AuditCase {
            case_set: "operational".to_owned(),
            key: "known-pulldown-link-ref-tab-panic".to_owned(),
            label: String::new(),
            classes: Vec::new(),
            cmark_extensions: Vec::new(),
            source: LINK_REF_TAB_REPRO.to_owned(),
            expected_html: None,
        };
        let err = Document::parse(&case.source).expect_err("known issue stays contained");
        assert_eq!(classify_parse_error(&case, &err), "upstream-panic");
    }

    #[test]
    fn spec_html_normalization_keeps_internal_whitespace() {
        assert_ne!(
            normalize_spec_html("<p>a\nb</p>\n"),
            normalize_spec_html("<p>a b</p>\n")
        );
        assert_eq!(normalize_spec_html("<p>a</p>"), "<p>a</p>\n");
    }

    #[test]
    fn parser_audit_fixture_covers_expected_constructs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/parser-audit");
        let mut combined = String::new();
        for path in collect_markdown_files(&root).expect("fixture files are readable") {
            combined.push_str(&fs::read_to_string(path).expect("fixture file is readable"));
            combined.push('\n');
        }

        assert!(combined.contains("| construct | status |"));
        assert!(combined.contains("- [ ] unchecked task"));
        assert!(combined.contains("www.example.com"));
        assert!(combined.contains("<div data-kind=\"raw\">"));
        assert!(combined.contains("{#heading .fixture}"));
        assert!(combined.contains(":::{note}"));
        assert!(combined.contains("::: {.warning}"));
    }

    fn test_case(case_set: &str, key: &str, label: &str) -> AuditCase {
        AuditCase {
            case_set: case_set.to_owned(),
            key: key.to_owned(),
            label: label.to_owned(),
            classes: Vec::new(),
            cmark_extensions: Vec::new(),
            source: String::new(),
            expected_html: None,
        }
    }
}
