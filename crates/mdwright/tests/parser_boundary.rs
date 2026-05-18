#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration tests: process failures should surface as panics"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PULLDOWN_PANIC_REPRO: &[u8] = b"- [n]:Z\r\n\t\t";

fn mdwright_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mdwright"))
}

fn write_repro(dir: &Path) -> PathBuf {
    let path = dir.join("parser-panic.md");
    fs::write(&path, PULLDOWN_PANIC_REPRO).expect("write parser panic repro");
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(mdwright_bin()).args(args).output().expect("run mdwright")
}

#[test]
fn cli_file_commands_report_parse_error_without_panic() {
    let temp = tempfile::tempdir().expect("tempdir");
    let path = write_repro(temp.path());
    let path_arg = path.to_string_lossy();

    for command in ["check", "fmt-check", "fmt", "fix", "render"] {
        let before = fs::read(&path).expect("read before");
        let output = run(&[command, &path_arg]);
        let after = fs::read(&path).expect("read after");
        assert_eq!(before, after, "{command} must leave parser-panic input unchanged");
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command} should return command/input error status; stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Markdown parser failed"),
            "{command} should report controlled parse error, got stderr:\n{stderr}"
        );
        assert!(
            !stderr.contains("panicked at") && !stderr.contains("thread '"),
            "{command} leaked a panic report:\n{stderr}"
        );
    }
}
