#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "test harness; assertions surface as panics"
)]

//! CI gate: `docs/src/configuration.md` must match what `cargo xtask
//! doc-config` produces from the current `SCHEMA_FIELDS` table. Drift
//! fails the build; fix by running the xtask.

use std::path::PathBuf;

#[test]
fn config_docs_in_sync() {
    let workspace = workspace_root();
    let drift = xtask::config_docs::check(&workspace).expect("xtask::config_docs::check ran");
    if !drift.is_empty() {
        let mut msg = String::from("config docs drifted; run `cargo xtask doc-config` to fix:\n");
        for d in &drift {
            msg.push_str("  - ");
            msg.push_str(&d.path.display().to_string());
            msg.push('\n');
        }
        panic!("{msg}");
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
