#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "test harness; assertions surface as panics"
)]

//! CI gate: every `rev: vX.Y.Z` and `@vX.Y.Z` pin in the integration
//! docs must match `Cargo.toml`'s `[workspace.package].version`. Drift fails the
//! build; fix by running `cargo xtask bump-docs-version`.

use std::path::PathBuf;

#[test]
fn integration_versions_in_sync() {
    let workspace = workspace_root();
    let drift = xtask::version_refs::check(&workspace).expect("xtask::version_refs::check ran");
    if !drift.is_empty() {
        let mut msg = String::from("integration version pins drifted; run `cargo xtask bump-docs-version` to fix:\n");
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
