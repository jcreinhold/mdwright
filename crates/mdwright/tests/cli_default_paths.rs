#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "test harness; assertions surface as panics"
)]

use std::fs;
use std::process::Command;

fn mdwright() -> &'static str {
    env!("CARGO_BIN_EXE_mdwright")
}

#[test]
fn check_without_paths_scans_current_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("note.md"), "See https://example.com.\n").expect("write markdown");

    let out = Command::new(mdwright())
        .args(["check", "--format=json"])
        .current_dir(dir.path())
        .output()
        .expect("run mdwright check");

    assert!(
        out.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    assert!(stdout.contains("note.md"), "missing scanned file path: {stdout}");
    assert!(
        stdout.contains("\"name\":\"bare-url\""),
        "missing bare-url diagnostic: {stdout}"
    );
}

#[test]
fn fmt_check_without_paths_scans_current_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join(".mdwright.toml"), "[fmt]\nwrap = \"no\"\n").expect("write config");
    fs::write(dir.path().join("note.md"), "alpha\nbeta\n").expect("write markdown");

    let out = Command::new(mdwright())
        .arg("fmt-check")
        .current_dir(dir.path())
        .output()
        .expect("run mdwright fmt-check");

    assert_eq!(
        out.status.code(),
        Some(1),
        "fmt-check should scan . and report a formatting change\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
