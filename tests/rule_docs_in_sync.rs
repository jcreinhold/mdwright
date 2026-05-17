#![allow(
    clippy::panic,
    clippy::expect_used,
    reason = "test harness; assertions surface as panics"
)]

//! CI gate: `docs/rules/<name>.md` and `docs/rules/index.md` must
//! match what `cargo xtask doc-rules` produces from the current rule
//! metadata. Drift fails the build; fix by running the xtask.

use std::path::PathBuf;

#[test]
fn rule_docs_in_sync() {
    let workspace = workspace_root();
    let drift = xtask::check(&workspace).expect("xtask::check ran");
    if !drift.is_empty() {
        let mut msg = String::from("rule docs drifted; run `cargo xtask doc-rules` to fix:\n");
        for d in &drift {
            msg.push_str("  - ");
            msg.push_str(&d.path.display().to_string());
            msg.push('\n');
        }
        panic!("{msg}");
    }
}

fn workspace_root() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
}
