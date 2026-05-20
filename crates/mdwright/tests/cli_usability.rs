#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration tests: process failures should surface as panics"
)]

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const PARSER_PANIC_REPRO: &[u8] = b"- [n]:Z\r\n\t\t";

fn mdwright() -> &'static str {
    env!("CARGO_BIN_EXE_mdwright")
}

fn output_text(output: &Output) -> (String, String) {
    (
        String::from_utf8(output.stdout.clone()).expect("utf8 stdout"),
        String::from_utf8(output.stderr.clone()).expect("utf8 stderr"),
    )
}

fn command(args: &[&str]) -> Command {
    let mut command = Command::new(mdwright());
    command.args(args);
    command
}

fn command_in(dir: &Path, args: &[&str]) -> Output {
    command(args).current_dir(dir).output().expect("run mdwright")
}

fn command_with_stdin(dir: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = command(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mdwright");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait mdwright")
}

fn write_wrap_no_config(dir: &Path) {
    fs::write(dir.join(".mdwright.toml"), "[fmt]\nwrap = \"no\"\n").expect("write config");
}

fn contains_ansi(text: &str) -> bool {
    text.contains("\u{1b}[")
}

fn has_option(help: &str, option: &str) -> bool {
    help.lines().any(|line| line.split_whitespace().next() == Some(option))
}

#[test]
fn help_surfaces_familiar_verbs_and_fmt_check_is_check_oriented() {
    let top = command(&["--help"]).output().expect("top-level help");
    let (top_stdout, _) = output_text(&top);
    for verb in ["check", "fix", "fmt", "fmt-check", "list-rules", "explain", "render"] {
        assert!(
            top_stdout.contains(verb),
            "top-level help missing {verb}:\n{top_stdout}"
        );
    }

    let check = command(&["check", "--help"]).output().expect("check help");
    let (check_stdout, _) = output_text(&check);
    assert!(
        check_stdout.contains('`') && check_stdout.contains('.') && check_stdout.contains('-'),
        "check help should document default path and explicit stdin:\n{check_stdout}"
    );

    let fmt = command(&["fmt", "--help"]).output().expect("fmt help");
    let (fmt_stdout, _) = output_text(&fmt);
    assert!(
        has_option(&fmt_stdout, "--range"),
        "fmt help should expose range formatting"
    );

    let fmt_check = command(&["fmt-check", "--help"]).output().expect("fmt-check help");
    let (fmt_check_stdout, _) = output_text(&fmt_check);
    assert!(
        !has_option(&fmt_check_stdout, "--check"),
        "fmt-check help should not expose redundant --check"
    );
    assert!(
        !has_option(&fmt_check_stdout, "--range"),
        "fmt-check help should not expose range formatting"
    );
    assert!(
        has_option(&fmt_check_stdout, "--diff"),
        "fmt-check help should expose --diff"
    );
}

#[test]
fn check_exit_codes_support_human_and_ci_workflows() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("note.md"), "See https://example.com.\n").expect("write markdown");

    let human = command_in(dir.path(), &["check", "--format=compact"]);
    let (human_stdout, human_stderr) = output_text(&human);
    assert_eq!(
        human.status.code(),
        Some(0),
        "stdout:\n{human_stdout}\nstderr:\n{human_stderr}"
    );
    assert!(
        human_stdout.contains("bare-url"),
        "human check should report diagnostics"
    );

    let ci = command_in(dir.path(), &["check", "--check", "--format=compact"]);
    let (ci_stdout, ci_stderr) = output_text(&ci);
    assert_eq!(ci.status.code(), Some(1), "stdout:\n{ci_stdout}\nstderr:\n{ci_stderr}");
    assert!(ci_stdout.contains("bare-url"), "CI check should report diagnostics");
}

#[test]
fn fmt_check_reports_drift_without_writing_or_polluting_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrap_no_config(dir.path());
    let note = dir.path().join("note.md");
    fs::write(&note, "alpha\nbeta\n").expect("write markdown");

    let out = command_in(dir.path(), &["fmt-check"]);
    let (stdout, stderr) = output_text(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.is_empty(),
        "fmt-check without --diff should keep stdout clean: {stdout:?}"
    );
    assert!(
        !stderr.trim().is_empty(),
        "fmt-check should explain formatting drift on stderr"
    );
    assert_eq!(fs::read_to_string(&note).expect("read markdown"), "alpha\nbeta\n");
}

#[test]
fn fmt_check_diff_prints_reviewable_diff_without_mutating() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrap_no_config(dir.path());
    let note = dir.path().join("note.md");
    fs::write(&note, "alpha\nbeta\n").expect("write markdown");

    let out = command_in(dir.path(), &["fmt-check", "--diff"]);
    let (stdout, stderr) = output_text(&out);
    assert_eq!(out.status.code(), Some(1), "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.contains("--- a/./note.md"), "missing diff header:\n{stdout}");
    assert!(stdout.contains("+alpha beta"), "missing formatted line:\n{stdout}");
    assert!(
        stderr.is_empty(),
        "fmt-check --diff should keep stderr clean: {stderr:?}"
    );
    assert_eq!(fs::read_to_string(&note).expect("read markdown"), "alpha\nbeta\n");
}

#[test]
fn fmt_diff_reviews_without_mutating_and_fmt_mutates_markdown_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrap_no_config(dir.path());
    let note = dir.path().join("note.md");
    let text = dir.path().join("note.txt");
    fs::write(&note, "alpha\nbeta\n").expect("write markdown");
    fs::write(&text, "alpha\nbeta\n").expect("write text");

    let diff = command_in(dir.path(), &["fmt", "--diff"]);
    let (diff_stdout, diff_stderr) = output_text(&diff);
    assert_eq!(
        diff.status.code(),
        Some(0),
        "stdout:\n{diff_stdout}\nstderr:\n{diff_stderr}"
    );
    assert!(
        diff_stdout.contains("+alpha beta"),
        "fmt --diff should show markdown changes"
    );
    assert_eq!(fs::read_to_string(&note).expect("read markdown"), "alpha\nbeta\n");
    assert_eq!(fs::read_to_string(&text).expect("read text"), "alpha\nbeta\n");

    let fmt = command_in(dir.path(), &["fmt"]);
    let (fmt_stdout, fmt_stderr) = output_text(&fmt);
    assert_eq!(
        fmt.status.code(),
        Some(0),
        "stdout:\n{fmt_stdout}\nstderr:\n{fmt_stderr}"
    );
    assert_eq!(fs::read_to_string(&note).expect("read markdown"), "alpha beta\n");
    assert_eq!(fs::read_to_string(&text).expect("read text"), "alpha\nbeta\n");
}

#[test]
fn explicit_stdin_has_clear_check_and_format_check_contracts() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrap_no_config(dir.path());

    let lint = command_with_stdin(
        dir.path(),
        &["check", "--format=json", "-"],
        "See https://example.com.\n",
    );
    let (lint_stdout, lint_stderr) = output_text(&lint);
    assert_eq!(
        lint.status.code(),
        Some(0),
        "stdout:\n{lint_stdout}\nstderr:\n{lint_stderr}"
    );
    assert!(
        lint_stdout.contains("\"path\":\"<stdin>\""),
        "stdin lint should report <stdin>"
    );

    let check = command_with_stdin(dir.path(), &["fmt-check", "-"], "alpha\nbeta\n");
    let (check_stdout, check_stderr) = output_text(&check);
    assert_eq!(
        check.status.code(),
        Some(1),
        "stdout:\n{check_stdout}\nstderr:\n{check_stderr}"
    );
    assert!(
        check_stdout.is_empty(),
        "fmt-check stdin should not write formatted markdown to stdout"
    );
    assert!(
        !check_stderr.trim().is_empty(),
        "fmt-check stdin should explain formatting drift"
    );

    let diff = command_with_stdin(dir.path(), &["fmt-check", "--diff", "-"], "alpha\nbeta\n");
    let (diff_stdout, diff_stderr) = output_text(&diff);
    assert_eq!(
        diff.status.code(),
        Some(1),
        "stdout:\n{diff_stdout}\nstderr:\n{diff_stderr}"
    );
    assert!(diff_stdout.contains("<stdin>"), "stdin diff should use stdin label");
    assert!(
        diff_stdout.contains("+alpha beta"),
        "stdin diff should show formatted line"
    );
}

#[test]
fn errors_are_actionable_and_do_not_leak_internal_types() {
    let missing = command(&["check", "missing.md"]).output().expect("missing path");
    let (_, missing_stderr) = output_text(&missing);
    assert_eq!(missing.status.code(), Some(2), "stderr:\n{missing_stderr}");
    assert!(missing_stderr.contains("path does not exist: missing.md"));

    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join(".mdwright.toml"), "[fmt]\nwrap = false\n").expect("write config");
    let bad_config = command_in(dir.path(), &["fmt-check"]);
    let (_, bad_stderr) = output_text(&bad_config);
    assert_eq!(bad_config.status.code(), Some(2), "stderr:\n{bad_stderr}");
    assert!(
        bad_stderr.contains(".mdwright.toml"),
        "config error should mention source path"
    );
    assert!(
        bad_stderr.contains("wrap"),
        "config error should identify the bad key:\n{bad_stderr}"
    );
    assert!(
        !bad_stderr.contains("WrapSchema") && !bad_stderr.contains("untagged enum"),
        "config error leaked implementation detail:\n{bad_stderr}"
    );

    let parser_dir = tempfile::tempdir().expect("tempdir");
    let parser_path = parser_dir.path().join("parser-panic.md");
    fs::write(&parser_path, PARSER_PANIC_REPRO).expect("write parser repro");
    let parser = command(&["fmt-check", parser_path.to_str().expect("utf8 path")])
        .output()
        .expect("parser boundary");
    let (_, parser_stderr) = output_text(&parser);
    assert_eq!(parser.status.code(), Some(2), "stderr:\n{parser_stderr}");
    assert!(parser_stderr.contains("Markdown parser failed"));
    assert!(!parser_stderr.contains("panicked at") && !parser_stderr.contains("thread '"));
}

#[test]
fn color_policy_is_predictable_for_humans_and_tools() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("note.md"), "See https://example.com.\n").expect("write markdown");

    let default = command_in(dir.path(), &["check", "--format=pretty"]);
    let (default_stdout, _) = output_text(&default);
    assert!(
        !contains_ansi(&default_stdout),
        "captured auto-color output should be plain"
    );

    let never = command_in(dir.path(), &["check", "--format=pretty", "--color=never"]);
    let (never_stdout, _) = output_text(&never);
    assert!(!contains_ansi(&never_stdout), "--color=never output should be plain");

    let always = command_in(dir.path(), &["check", "--format=pretty", "--color=always"]);
    let (always_stdout, _) = output_text(&always);
    assert!(contains_ansi(&always_stdout), "--color=always should emit ANSI");

    let json = command_in(dir.path(), &["check", "--format=json", "--color=always"]);
    let (json_stdout, _) = output_text(&json);
    let first_json: Value = serde_json::from_str(json_stdout.lines().next().expect("json record")).expect("valid json");
    let rule_name = first_json
        .get("rule")
        .and_then(|rule| rule.get("name"))
        .and_then(Value::as_str);
    assert_eq!(rule_name, Some("bare-url"));
    assert!(!contains_ansi(&json_stdout), "JSON output must not contain ANSI");

    let compact = command_in(dir.path(), &["check", "--format=compact", "--color=always"]);
    let (compact_stdout, _) = output_text(&compact);
    assert!(!contains_ansi(&compact_stdout), "compact output must not contain ANSI");
}
