#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test harness; assertions surface as panics"
)]

//! End-to-end smoke test for the integration surfaces published in
//! `.pre-commit-hooks.yaml` and `action.yml`.
//!
//! The test bypasses the `pre-commit` framework and `act` entirely:
//! it invokes the locally-built `mdwright` binary against the
//! fixtures in `examples/downstream/docs/` and asserts the exit
//! codes plus the rule names that surface in stderr. That makes the
//! test hermetic (no `pre-commit` / `act` / network) while still
//! catching the regressions a hook contract would catch — a renamed
//! subcommand, a renamed rule, or a changed exit-code mapping.

use std::path::PathBuf;
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples")
        .join("downstream")
        .join("docs")
}

fn mdwright_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mdwright"))
}

#[test]
fn good_doc_passes_check() {
    let out = Command::new(mdwright_bin())
        .args(["check", "--check"])
        .arg(fixtures_dir().join("good.md"))
        .output()
        .expect("invoke mdwright check");
    assert!(
        out.status.success(),
        "`mdwright check --check good.md` exited {:?}; stderr was:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn good_doc_passes_fmt_check() {
    let out = Command::new(mdwright_bin())
        .arg("fmt-check")
        .arg(fixtures_dir().join("good.md"))
        .output()
        .expect("invoke mdwright fmt-check");
    assert!(
        out.status.success(),
        "`mdwright fmt-check good.md` exited {:?}; stderr was:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn bad_doc_fails_check_with_named_rules() {
    let out = Command::new(mdwright_bin())
        .args(["check", "--check"])
        .arg(fixtures_dir().join("bad.md"))
        .output()
        .expect("invoke mdwright check");

    assert!(
        !out.status.success(),
        "`mdwright check --check bad.md` unexpectedly exited 0",
    );

    // Diagnostics in pretty mode go to stdout; the trailing summary
    // line goes there too. stderr stays clean for tracing output.
    let stdout = String::from_utf8_lossy(&out.stdout);
    for rule in ["bare-url", "math/unbalanced-delim"] {
        assert!(
            stdout.contains(rule),
            "expected rule `{rule}` to fire on bad.md; stdout was:\n{stdout}",
        );
    }
}
