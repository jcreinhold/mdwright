use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

const REPORT_JSON: &str = "release-evidence.json";
const REPORT_MD: &str = "release-evidence.md";

const RELEASE_CLAIM: &str = "round-trip-safe Markdown formatter and linter with classified GFM/parser divergences and an opt-in mdformat-compatible style profile";

#[derive(Clone, Debug, Serialize)]
struct ReleaseEvidenceReport {
    release_claim: String,
    git: GitInfo,
    tools: Vec<ToolInfo>,
    evidence: Vec<EvidenceItem>,
    accepted_divergences: Vec<AcceptedDivergence>,
    blockers: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct GitInfo {
    commit: String,
    branch: String,
    dirty: bool,
    status: String,
}

#[derive(Clone, Debug, Serialize)]
struct ToolInfo {
    name: String,
    command: String,
    available: bool,
    output: String,
}

#[derive(Clone, Debug, Serialize)]
struct EvidenceItem {
    name: String,
    status: EvidenceStatus,
    required: bool,
    path: Option<String>,
    command: String,
    summary: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum EvidenceStatus {
    Present,
    Missing,
    Manual,
}

#[derive(Clone, Debug, Serialize)]
struct AcceptedDivergence {
    topic: String,
    source: String,
}

pub fn run(workspace: &Path, output: &Path) -> Result<bool> {
    let report = collect(workspace)?;
    write_reports(output, &report)?;
    print_summary(output, &report);
    Ok(report.blockers.is_empty())
}

fn collect(workspace: &Path) -> Result<ReleaseEvidenceReport> {
    let git = git_info(workspace);
    let mut evidence = Vec::new();
    let mut blockers = Vec::new();

    evidence.push(manual_item(
        "workspace fast checks",
        "Run the fast local gate and paste its output into the release notes if any command fails.",
        "cargo check --workspace --all-targets && cargo nextest run --workspace --no-fail-fast && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check && cargo doc --workspace --no-deps && mdbook build docs/",
        true,
    ));
    blockers.push("manual evidence missing: workspace fast checks".to_owned());

    collect_json_item(
        workspace,
        &mut evidence,
        &mut blockers,
        JsonEvidence {
            name: "parser audit",
            path: "target/mdwright/parser-audit/parser-audit.json",
            command: "cargo xtask parser-audit --case-set all --ensure-tools --include-comrak",
            required: true,
            summarise: summarise_parser_audit,
        },
    );
    collect_json_item(
        workspace,
        &mut evidence,
        &mut blockers,
        JsonEvidence {
            name: "mdformat parity",
            path: "target/mdwright/parity/mdformat-parity.json",
            command: "cargo xtask mdformat-parity --corpus-root docs --corpus-name mdwright-docs --mdwright-config .mdwright.toml --mdformat-config xtask/fixtures/mdformat-parity/mdformat.toml",
            required: true,
            summarise: summarise_mdformat_parity,
        },
    );
    collect_json_item(
        workspace,
        &mut evidence,
        &mut blockers,
        JsonEvidence {
            name: "production soak",
            path: "target/mdwright/production-soak/production-soak.json",
            command: "cargo xtask production-soak --corpus-root <KAN-CHECKOUT> --output target/mdwright/production-soak",
            required: true,
            summarise: summarise_production_soak,
        },
    );
    collect_json_item(
        workspace,
        &mut evidence,
        &mut blockers,
        JsonEvidence {
            name: "package and install dry run",
            path: "target/mdwright/package-dry-run/report.json",
            command: "Run the package/install dry-run commands from docs/src/reference/release-evidence.md.",
            required: true,
            summarise: summarise_package_dry_run,
        },
    );

    evidence.push(manual_item(
        "fuzz corpus replay",
        "Replay all five fuzz corpora and record the commands plus outcomes in target/mdwright/release/fuzz-replay.md.",
        "cargo +nightly fuzz run fuzz_parse_format -- -runs=0; cargo +nightly fuzz run fuzz_idempotence -- -runs=0; cargo +nightly fuzz run fuzz_structured_idempotence -- -runs=0; cargo +nightly fuzz run fuzz_lint -- -runs=0; cargo +nightly fuzz run fuzz_verbatim_identity -- -runs=0",
        true,
    ));
    blockers.push("manual evidence missing: fuzz corpus replay".to_owned());
    evidence.push(manual_item(
        "sustained fuzz rounds",
        "Record sustained fuzz rounds in target/mdwright/release/fuzz-sustained.md.",
        "cargo +nightly fuzz run <target> -- -max_total_time=600",
        true,
    ));
    blockers.push("manual evidence missing: sustained fuzz rounds".to_owned());
    evidence.push(manual_item(
        "benchmark comparison",
        "Record Criterion preserve/mdformat profile comparison in target/mdwright/release/benchmarks.md.",
        "cargo bench -p mdwright --bench format_bench --bench lint_bench -- --baseline pre-parser-boundary",
        true,
    ));
    blockers.push("manual evidence missing: benchmark comparison".to_owned());

    for manual in [
        ("target/mdwright/release/fuzz-replay.md", "fuzz corpus replay"),
        ("target/mdwright/release/fuzz-sustained.md", "sustained fuzz rounds"),
        ("target/mdwright/release/benchmarks.md", "benchmark comparison"),
        ("target/mdwright/release/fast-checks.md", "workspace fast checks"),
    ] {
        if workspace.join(manual.0).is_file() {
            mark_manual_present(&mut evidence, manual.1, &workspace.join(manual.0));
            blockers.retain(|blocker| blocker != &format!("manual evidence missing: {}", manual.1));
        }
    }

    if git.dirty {
        blockers.push("git worktree is dirty".to_owned());
    }

    Ok(ReleaseEvidenceReport {
        release_claim: RELEASE_CLAIM.to_owned(),
        git,
        tools: tool_info(workspace),
        evidence,
        accepted_divergences: vec![
            AcceptedDivergence {
                topic: "parser backend drift".to_owned(),
                source: "docs/architecture/parser-backend-audit.md".to_owned(),
            },
            AcceptedDivergence {
                topic: "mdformat byte-output differences".to_owned(),
                source: "docs/architecture/mdformat-parity.md".to_owned(),
            },
            AcceptedDivergence {
                topic: "GFM snapshot deviations".to_owned(),
                source: "docs/src/deviations.md".to_owned(),
            },
        ],
        blockers,
    })
}

struct JsonEvidence {
    name: &'static str,
    path: &'static str,
    command: &'static str,
    required: bool,
    summarise: fn(&Value) -> Vec<String>,
}

fn collect_json_item(
    workspace: &Path,
    evidence: &mut Vec<EvidenceItem>,
    blockers: &mut Vec<String>,
    item: JsonEvidence,
) {
    let path = workspace.join(item.path);
    match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(json) => evidence.push(EvidenceItem {
                name: item.name.to_owned(),
                status: EvidenceStatus::Present,
                required: item.required,
                path: Some(item.path.to_owned()),
                command: item.command.to_owned(),
                summary: (item.summarise)(&json),
            }),
            Err(err) => {
                evidence.push(EvidenceItem {
                    name: item.name.to_owned(),
                    status: EvidenceStatus::Missing,
                    required: item.required,
                    path: Some(item.path.to_owned()),
                    command: item.command.to_owned(),
                    summary: vec![format!("report exists but is not valid JSON: {err}")],
                });
                if item.required {
                    blockers.push(format!("invalid evidence report: {}", item.path));
                }
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            evidence.push(EvidenceItem {
                name: item.name.to_owned(),
                status: EvidenceStatus::Missing,
                required: item.required,
                path: Some(item.path.to_owned()),
                command: item.command.to_owned(),
                summary: vec!["report file is missing".to_owned()],
            });
            if item.required {
                blockers.push(format!("missing evidence report: {}", item.path));
            }
        }
        Err(err) => {
            evidence.push(EvidenceItem {
                name: item.name.to_owned(),
                status: EvidenceStatus::Missing,
                required: item.required,
                path: Some(item.path.to_owned()),
                command: item.command.to_owned(),
                summary: vec![format!("could not read report: {err}")],
            });
            if item.required {
                blockers.push(format!("unreadable evidence report: {}", item.path));
            }
        }
    }
}

fn manual_item(name: &str, summary: &str, command: &str, required: bool) -> EvidenceItem {
    EvidenceItem {
        name: name.to_owned(),
        status: EvidenceStatus::Manual,
        required,
        path: None,
        command: command.to_owned(),
        summary: vec![summary.to_owned()],
    }
}

fn mark_manual_present(evidence: &mut [EvidenceItem], name: &str, path: &Path) {
    if let Some(item) = evidence.iter_mut().find(|item| item.name == name) {
        item.status = EvidenceStatus::Present;
        item.path = Some(path.display().to_string());
        item.summary = vec!["manual evidence note is present".to_owned()];
    }
}

fn summarise_parser_audit(json: &Value) -> Vec<String> {
    let stats = &json["stats"];
    vec![
        field_summary(stats, "cases", "cases"),
        field_summary(stats, "pulldown_html_mismatches", "HTML mismatches"),
        field_summary(stats, "sourcepos_differences", "sourcepos differences"),
        field_summary(stats, "unclassified_differences", "unclassified differences"),
        field_summary(stats, "mitigation_rows_observed", "mitigation rows observed"),
        format!("failures: {}", json["failures"].as_array().map_or(0, Vec::len)),
    ]
}

fn summarise_mdformat_parity(json: &Value) -> Vec<String> {
    let stats = &json["stats"];
    vec![
        field_summary(stats, "markdown_files", "markdown files"),
        field_summary(stats, "output_different_files", "mdwright/mdformat different files"),
        field_summary(stats, "unclassified_differences", "unclassified differences"),
        field_summary(stats, "mdwright_semantic_drift_failures", "mdwright semantic drift"),
        format!("failures: {}", json["failures"].as_array().map_or(0, Vec::len)),
    ]
}

fn summarise_production_soak(json: &Value) -> Vec<String> {
    vec![
        value_summary(json, "success", "success"),
        value_summary(json, "files_scanned", "files scanned"),
        value_summary(json, "parse_errors", "parse errors"),
        value_summary(json, "validation_errors", "validation errors"),
        value_summary(json, "idempotence_failures", "idempotence failures"),
        value_summary(json, "fmt_check_disagreements", "fmt-check disagreements"),
    ]
}

fn summarise_package_dry_run(json: &Value) -> Vec<String> {
    vec![
        value_summary(json, "package_checks", "package checks"),
        value_summary(json, "install_root", "install root"),
        value_summary(json, "downstream_project", "downstream project"),
        value_summary(json, "dist_artifacts", "dist artifacts"),
        value_summary(json, "remaining_blockers", "remaining blockers"),
    ]
}

fn field_summary(stats: &Value, field: &str, label: &str) -> String {
    value_summary(stats, field, label)
}

fn value_summary(json: &Value, field: &str, label: &str) -> String {
    match json.get(field) {
        Some(Value::Bool(value)) => format!("{label}: {}", yes_no(*value)),
        Some(Value::Number(value)) => format!("{label}: {value}"),
        Some(Value::String(value)) => format!("{label}: {value}"),
        Some(Value::Array(value)) => format!("{label}: {} item(s)", value.len()),
        Some(Value::Object(value)) => format!("{label}: {} field(s)", value.len()),
        Some(_) => format!("{label}: present"),
        None => format!("{label}: unknown"),
    }
}

fn git_info(workspace: &Path) -> GitInfo {
    let commit = command_stdout(workspace, "git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let branch =
        command_stdout(workspace, "git", &["branch", "--show-current"]).unwrap_or_else(|| "unknown".to_owned());
    let status = command_stdout(workspace, "git", &["status", "--short"]).unwrap_or_default();
    GitInfo {
        commit,
        branch,
        dirty: !status.trim().is_empty(),
        status,
    }
}

fn tool_info(workspace: &Path) -> Vec<ToolInfo> {
    [
        ("rustc", "rustc", &["--version"][..]),
        ("cargo", "cargo", &["--version"][..]),
        ("cargo-nextest", "cargo", &["nextest", "--version"][..]),
        ("cargo-fuzz", "cargo", &["fuzz", "--version"][..]),
        ("mdbook", "mdbook", &["--version"][..]),
    ]
    .into_iter()
    .map(|(name, program, args)| {
        let output = command_stdout(workspace, program, args);
        ToolInfo {
            name: name.to_owned(),
            command: std::iter::once(program)
                .chain(args.iter().copied())
                .collect::<Vec<_>>()
                .join(" "),
            available: output.is_some(),
            output: output.unwrap_or_else(|| "not available".to_owned()),
        }
    })
    .collect()
}

fn command_stdout(workspace: &Path, program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).current_dir(workspace).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn write_reports(output: &Path, report: &ReleaseEvidenceReport) -> Result<()> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    let json_path = output.join(REPORT_JSON);
    let md_path = output.join(REPORT_MD);
    fs::write(
        &json_path,
        serde_json::to_string_pretty(report).context("serialize release evidence")?,
    )
    .with_context(|| format!("write {}", json_path.display()))?;
    fs::write(&md_path, markdown_report(report)).with_context(|| format!("write {}", md_path.display()))?;
    Ok(())
}

fn markdown_report(report: &ReleaseEvidenceReport) -> String {
    let mut out = String::from("# Release evidence\n\n");
    out.push_str(&format!("Release claim: {}.\n\n", report.release_claim));
    out.push_str("## Candidate\n\n");
    out.push_str(&format!("- Commit: `{}`\n", report.git.commit));
    out.push_str(&format!("- Branch: `{}`\n", report.git.branch));
    out.push_str(&format!("- Dirty worktree: `{}`\n", yes_no(report.git.dirty)));
    if report.git.dirty && !report.git.status.trim().is_empty() {
        out.push_str("\n```text\n");
        out.push_str(report.git.status.trim());
        out.push_str("\n```\n");
    }

    out.push_str("\n## Tool Versions\n\n");
    out.push_str("| Tool | Available | Output |\n");
    out.push_str("| --- | --- | --- |\n");
    for tool in &report.tools {
        out.push_str(&format!(
            "| `{}` | {} | `{}` |\n",
            tool.name,
            yes_no(tool.available),
            tool.output.replace('`', "\\`").replace('\n', "<br>")
        ));
    }

    out.push_str("\n## Evidence\n\n");
    out.push_str("| Gate | Status | Required | Report | Summary |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for item in &report.evidence {
        out.push_str(&format!(
            "| {} | `{}` | {} | {} | {} |\n",
            item.name,
            status_label(&item.status),
            yes_no(item.required),
            item.path
                .as_ref()
                .map(|path| format!("`{path}`"))
                .unwrap_or_else(|| "manual note".to_owned()),
            item.summary.join("<br>")
        ));
    }

    out.push_str("\n## Accepted Divergences\n\n");
    for divergence in &report.accepted_divergences {
        out.push_str(&format!("- {}: `{}`\n", divergence.topic, divergence.source));
    }

    out.push_str("\n## Blockers\n\n");
    if report.blockers.is_empty() {
        out.push_str("None.\n");
    } else {
        for blocker in &report.blockers {
            out.push_str(&format!("- {blocker}\n"));
        }
    }
    out
}

fn status_label(status: &EvidenceStatus) -> &'static str {
    match status {
        EvidenceStatus::Present => "present",
        EvidenceStatus::Missing => "missing",
        EvidenceStatus::Manual => "manual",
    }
}

fn print_summary(output: &Path, report: &ReleaseEvidenceReport) {
    println!("release evidence summary");
    println!("  claim: {}", report.release_claim);
    println!("  commit: {}", report.git.commit);
    println!("  dirty worktree: {}", yes_no(report.git.dirty));
    println!("  evidence items: {}", report.evidence.len());
    println!("  blockers: {}", report.blockers.len());
    println!("  reports:");
    println!("    {}", output.join(REPORT_JSON).display());
    println!("    {}", output.join(REPORT_MD).display());
    if !report.blockers.is_empty() {
        println!("  blockers:");
        for blocker in &report.blockers {
            println!("    {blocker}");
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn missing_required_reports_are_blockers() {
        let dir = tempdir().expect("tempdir");
        let report = collect(dir.path()).expect("collect");
        assert!(
            report
                .blockers
                .iter()
                .any(|blocker| blocker.contains("parser-audit.json")),
            "missing parser audit must block release evidence"
        );
        assert!(
            markdown_report(&report).contains("missing evidence report"),
            "markdown should explain missing reports"
        );
    }

    #[test]
    fn parser_audit_summary_reads_stats() {
        let json = json!({
            "stats": {
                "cases": 673,
                "pulldown_html_mismatches": 15,
                "sourcepos_differences": 0,
                "unclassified_differences": 0,
                "mitigation_rows_observed": 0
            },
            "failures": []
        });
        let summary = summarise_parser_audit(&json);
        assert!(summary.contains(&"cases: 673".to_owned()));
        assert!(summary.contains(&"failures: 0".to_owned()));
    }

    #[test]
    fn write_reports_creates_json_and_markdown() {
        let dir = tempdir().expect("tempdir");
        let output = dir.path().join("release");
        let report = ReleaseEvidenceReport {
            release_claim: RELEASE_CLAIM.to_owned(),
            git: GitInfo {
                commit: "abc".to_owned(),
                branch: "main".to_owned(),
                dirty: false,
                status: String::new(),
            },
            tools: Vec::new(),
            evidence: Vec::new(),
            accepted_divergences: Vec::new(),
            blockers: Vec::new(),
        };
        write_reports(&output, &report).expect("write reports");
        assert!(output.join(REPORT_JSON).is_file());
        assert!(output.join(REPORT_MD).is_file());
        let md = fs::read_to_string(output.join(REPORT_MD)).expect("read markdown");
        assert!(md.contains("Release evidence"));
    }
}
