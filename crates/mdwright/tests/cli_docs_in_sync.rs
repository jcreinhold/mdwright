#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "test harness; assertions surface as panics"
)]

//! CI gate: `docs/src/reference/cli.md` must match what
//! `cargo xtask doc-cli` produces from clap's `--help` output. Drift
//! fails the build; fix by running the xtask.

use std::path::{Path, PathBuf};

#[test]
fn cli_docs_in_sync() {
    let workspace = workspace_root();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_mdwright"));
    let drift = xtask::cli_docs::check(&workspace, Some(bin.as_path())).expect("xtask::cli_docs::check ran");
    if !drift.is_empty() {
        let mut msg = String::from("CLI docs drifted; run `cargo xtask doc-cli` to fix:\n");
        for d in &drift {
            msg.push_str("  - ");
            msg.push_str(&d.path.display().to_string());
            msg.push('\n');
        }
        panic!("{msg}");
    }
}

fn workspace_root() -> PathBuf {
    let manifest: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../..")
}
