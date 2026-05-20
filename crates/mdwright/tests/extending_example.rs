#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test harness; assertions surface as panics"
)]

//! End-to-end test for `examples/extending/`: the sample downstream
//! crate compiles, registers `no-todo-in-prose` on top of the stdlib
//! via `mdwright::run_with_rules`, and the rule actually fires
//! against a fixture document.
//!
//! The test invokes `cargo run -p mdwright-extra-example` from the
//! workspace root rather than reaching into `target/debug/` directly,
//! so it works regardless of profile and stays portable across CI
//! runners.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    workspace_root()
        .join("examples")
        .join("extending")
        .join("fixtures")
        .join(name)
}

fn run_example(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .args(["run", "--quiet", "-p", "mdwright-extra-example", "--"])
        .args(args)
        .output()
        .expect("invoke cargo run -p mdwright-extra-example")
}

fn run_example_with_stdin(args: &[&str], stdin: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .args(["run", "--quiet", "-p", "mdwright-extra-example", "--"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mdwright-extra-example");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(stdin)
        .expect("write stdin");
    child.wait_with_output().expect("collect output")
}

#[test]
fn extra_rule_appears_in_list_rules() {
    let out = run_example(&["list-rules"]);
    assert!(
        out.status.success(),
        "list-rules exited {:?}; stderr was:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no-todo-in-prose"),
        "expected `no-todo-in-prose` in list-rules output; got:\n{stdout}",
    );
    assert!(
        stdout.contains("bare-url"),
        "expected stdlib rules (e.g. `bare-url`) to remain registered; got:\n{stdout}",
    );
}

#[test]
fn extra_rule_fires_on_fixture_file() {
    let path = fixture("has-todo.md");
    let arg = path.to_string_lossy().to_string();
    let out = run_example(&[
        "check",
        "--check",
        "--rules",
        "no-todo-in-prose",
        "--format",
        "compact",
        &arg,
    ]);
    assert!(
        !out.status.success(),
        "expected non-zero exit because the fixture trips the rule; stdout was:\n{}\nstderr was:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no-todo-in-prose"),
        "expected `no-todo-in-prose` diagnostic in stdout; got:\n{stdout}",
    );
    assert!(
        stdout.contains("literal `TODO` in prose"),
        "expected the rule's diagnostic message in stdout; got:\n{stdout}",
    );
}

#[test]
fn fenced_todo_is_not_flagged() {
    // Verifies the rule's prose-only contract: a TODO inside a code
    // fence is invisible to `prose_chunks()`, so the rule does not
    // fire on it.
    let body = b"```rust\n// TODO: hidden from prose-only rules.\n```\n";
    let out = run_example_with_stdin(
        &[
            "check",
            "--check",
            "--rules",
            "no-todo-in-prose",
            "--format",
            "compact",
            "-",
        ],
        body,
    );
    assert!(
        out.status.success(),
        "expected exit 0 (no diagnostics) for TODO inside a fenced block; \
         stdout was:\n{}\nstderr was:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn stdlib_rule_still_fires_through_extended_binary() {
    // Sanity: registering an extra rule does not displace stdlib
    // rules. `bare-url` lives in the stdlib and trips on this input.
    let out = run_example_with_stdin(
        &["check", "--rules", "bare-url", "--format", "compact", "-"],
        b"See https://example.com for details.\n",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("bare-url"),
        "expected `bare-url` diagnostic via the extended binary; got:\n{stdout}",
    );
}
